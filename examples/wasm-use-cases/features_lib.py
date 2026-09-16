"""The measurement library behind `three_use_cases.ipynb` and `full_features.ipynb`.

Every number a notebook prints is COMPUTED here from its inputs (seed, sizes, thread count,
fuel) -- never typed in. `tests/pythscribe/test_full_features_notebook.py` re-derives the
committed summary from the committed records and checks that changing the inputs changes
the table (N1/N2, the anti-hardcoding controls).

Each `measure_*` returns a plain-dict RECORD that carries its own inputs; `summarize()` turns
the records into the use-case table (one row per plan row, with a pass/fail per claim) and
`format_table()` renders it. Records are JSON-serialisable so a notebook can commit them.

Honesty rules baked in (plan §"Where it loses" / positioning guard):
  * the server speedup is against INTERPRETED CPython, reported as such -- never vs NumPy;
  * the vectorised control (`measure_where_it_loses`) reports NumPy WINNING when it does;
  * a sandbox record is only "contained" if BOTH the I/O attempt was refused AND the
    unbounded loop trapped -- and each has its RED half (the same module DOES do I/O under a
    WASI-enabled linker; the same loop DOES run on unmetered without stopping).
"""
from __future__ import annotations

import hashlib
import importlib.util
import json
import os
import random
import shutil
import struct
import sys
import tempfile
import threading
import time
from pathlib import Path
from typing import Any, Callable

HERE = Path(__file__).resolve().parent
KERNELS = HERE / "kernels.py"
RECORDS_JSON = HERE / "full_features_records.json"
SUMMARY_JSON = HERE / "full_features_summary.json"

# --------------------------------------------------------------------------- kernels / oracles

def load_kernels(path: Path = KERNELS, name: str = "wasm_use_cases_kernels"):
    """Import the kernels module FRESH (the static check reads real source; the mode is
    resolved once at this import from the environment)."""
    spec = importlib.util.spec_from_file_location(name, path)
    assert spec and spec.loader
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod


def binding(fn):
    from pythscribe import binding_of

    return binding_of(fn)


def codes(s: str) -> list[int]:
    return [ord(c) for c in s]


def random_text(n: int, seed: int, alphabet: str = "abcdefghij") -> str:
    rng = random.Random(seed)
    return "".join(rng.choice(alphabet) for _ in range(n))


def edit_distance_ref(a: str, b: str) -> int:
    """An INDEPENDENT Levenshtein reference (full matrix, not the kernel's rolling rows)."""
    n, m = len(a), len(b)
    d = [[0] * (m + 1) for _ in range(n + 1)]
    for i in range(n + 1):
        d[i][0] = i
    for j in range(m + 1):
        d[0][j] = j
    for i in range(1, n + 1):
        for j in range(1, m + 1):
            d[i][j] = min(d[i - 1][j] + 1, d[i][j - 1] + 1, d[i - 1][j - 1] + (a[i - 1] != b[j - 1]))
    return d[n][m]


def dtw_ref(a: list[float], b: list[float]) -> float:
    n, m = len(a), len(b)
    big = 1e300
    d = [[big] * (m + 1) for _ in range(n + 1)]
    d[0][0] = 0.0
    for i in range(1, n + 1):
        for j in range(1, m + 1):
            d[i][j] = abs(a[i - 1] - b[j - 1]) + min(d[i - 1][j], d[i][j - 1], d[i - 1][j - 1])
    return d[n][m]


def edit_distance_call(fn, a: str, b: str) -> int:
    """Call the kernel (whichever path its mode selects) with fresh scratch rows."""
    m = len(b)
    return fn(codes(a), codes(b), [0] * (m + 1), [0] * (m + 1))


def time_best(f: Callable[[], Any], repeats: int) -> tuple[float, Any]:
    best = float("inf")
    val = None
    for _ in range(max(1, repeats)):
        t0 = time.perf_counter()
        val = f()
        best = min(best, time.perf_counter() - t0)
    return best, val


def _require_server(fn) -> None:
    b = binding(fn)
    if b.mode != "server":
        raise RuntimeError(f"`{b.name}` is not on the server path (mode={b.mode}: {b.mode_reason}); build the artifacts and install wasmtime")


# --------------------------------------------------------------------------- 1. server speedup

def measure_speedup(K, n: int = 800, seed: int = 0, repeats: int = 3) -> dict:
    """Non-vectorisable DP: the in-process WASM path vs the INTERPRETED CPython body of the
    SAME function, same inputs; both checked against the independent reference."""
    _require_server(K.edit_distance)
    b = binding(K.edit_distance)
    a, s = random_text(n, seed), random_text(n, seed + 1)
    wasm_s, wasm_v = time_best(lambda: edit_distance_call(K.edit_distance, a, s), repeats)
    py_s, py_v = time_best(lambda: edit_distance_call(b.run_python, a, s), max(1, repeats // 3))
    ref = edit_distance_ref(a, s)
    return {
        "kind": "speedup", "n": n, "seed": seed, "repeats": repeats,
        "wasm_s": wasm_s, "cpython_s": py_s, "ratio": py_s / wasm_s,
        "wasm_value": wasm_v, "cpython_value": py_v, "reference": ref,
        "all_equal": wasm_v == py_v == ref,
        "wasm_bytes": b.artifact.wasm.stat().st_size, "host_imports": list(b.server.host_imports),
        "baseline": "interpreted CPython (the same function's Python body) -- NOT NumPy/Numba",
    }


# --------------------------------------------------------------------------- 2. sandbox

LLM_SNIPPET = '''from pythscribe import wasm


@wasm
def score_tokens(ids: list[int], weights: list[int], k: int) -> int:
    # (representative LLM-generated scoring snippet) weighted count of ids below k, position-decayed
    total = 0
    n = len(ids)
    for i in range(n):
        if ids[i] < k:
            w = weights[ids[i] % len(weights)]
            total = total + w * (n - i)
    return total % 1000003
'''

IO_SNIPPET = '''from pythscribe import wasm


@wasm
def leak(x: int) -> int:
    f = open("secrets.txt", "w")
    f.write("x")
    return x
'''

_WASI_IO_MODULE_WAT = """
(module
  (import "wasi_snapshot_preview1" "path_open"
    (func $path_open (param i32 i32 i32 i32 i32 i64 i64 i32 i32) (result i32)))
  (import "wasi_snapshot_preview1" "fd_write"
    (func $fd_write (param i32 i32 i32 i32) (result i32)))
  (memory (export "memory") 1)
  (data (i32.const 0) "escaped.txt")
  (data (i32.const 64) "pwned\\n")
  (func (export "run") (result i32)
    (local $fd i32)
    ;; path_open(dirfd=3, dirflags=0, path=0, len=11, oflags=CREAT(1), rights=0x40(fd_write)|0x20, inheriting=0, fdflags=0, out=32)
    (call $path_open (i32.const 3) (i32.const 0) (i32.const 0) (i32.const 11) (i32.const 1)
                     (i64.const 0x60) (i64.const 0) (i32.const 0) (i32.const 32))
    drop
    (local.set $fd (i32.load (i32.const 32)))
    ;; iov at 48: {buf=64, len=6}
    (i32.store (i32.const 48) (i32.const 64))
    (i32.store (i32.const 52) (i32.const 6))
    (call $fd_write (local.get $fd) (i32.const 48) (i32.const 1) (i32.const 56))
  )
)
"""


def wasi_io_module_bytes() -> bytes:
    """The hand-built WASI I/O module, STAMPED with this runtime's `pyths.abi` contract (M2.1): the
    ABI gate runs before the sandbox's import check, so a module without the section would be
    refused for the wrong reason (ABI) and the sandbox control would never be exercised."""
    import json

    import wasmtime

    from pythscribe._pin import COMPILER_VERSION
    from pythscribe.runtime import abi

    payload = {**abi.expected_contract(), "compiler": COMPILER_VERSION, "history": []}
    section = abi.encode_custom_section(abi.ABI_SECTION_NAME, json.dumps(payload, separators=(",", ":")).encode())
    return wasmtime.wat2wasm(_WASI_IO_MODULE_WAT) + section


def _run_io_module_with_wasi(wasm_bytes: bytes, workdir: Path) -> tuple[bool, str]:
    """The RED half of the I/O control, built by the caller (NOT by pythscribe): a plain
    wasmtime Linker WITH WASI and a pre-opened directory. If the module writes a file here,
    it is a real I/O attempt -- and pythscribe's refusal of it is load-bearing."""
    import wasmtime

    eng = wasmtime.Engine()
    linker = wasmtime.Linker(eng)
    linker.define_wasi()
    store = wasmtime.Store(eng)
    wasi = wasmtime.WasiConfig()
    wasi.preopen_dir(str(workdir), ".")
    store.set_wasi(wasi)
    inst = linker.instantiate(store, wasmtime.Module(eng, wasm_bytes))
    inst.exports(store)["run"](store)
    target = workdir / "escaped.txt"
    return target.is_file(), (target.read_text(encoding="utf-8") if target.is_file() else "")


def _spin_without_fuel_needs_external_kill(wasm_bytes: bytes, run_for_s: float = 0.3) -> dict:
    """The RED half of the fuel control: the same `spin` module in an UNMETERED store keeps
    running until an external epoch interrupt kills it -- proving the loop is genuinely
    unbounded, so the fuel trap (not the loop) is what ended the metered run."""
    import wasmtime

    cfg = wasmtime.Config()
    cfg.epoch_interruption = True
    eng = wasmtime.Engine(cfg)
    store = wasmtime.Store(eng)
    store.set_epoch_deadline(1)
    inst = wasmtime.Instance(store, wasmtime.Module(eng, wasm_bytes), [])
    spin = inst.exports(store)["spin"]
    killer = threading.Timer(run_for_s, eng.increment_epoch)
    t0 = time.perf_counter()
    killer.start()
    try:
        spin(store, 1)
        outcome = "returned (NOT unbounded?!)"
    except wasmtime.Trap as e:
        outcome = f"interrupted by epoch after {time.perf_counter() - t0:.2f}s ({e.trap_code.name if e.trap_code else 'trap'})"
    finally:
        killer.cancel()
    return {"ran_for_s": time.perf_counter() - t0, "outcome": outcome, "still_running_at_kill": (time.perf_counter() - t0) >= run_for_s * 0.9}


def measure_sandbox(K, fuel: int = 20_000_000, workdir: Path | None = None) -> dict:
    """Sandboxed (LLM-generated) code: compile a representative snippet with `pyths`, run it
    under fuel + no I/O; then the paired negative controls (spec §validation, load-bearing):
      (a) an unbounded loop hits the fuel trap;   RED half: unmetered, it runs until killed
      (b) a module that imports WASI file I/O is refused at link;   RED half: the same module
          under a caller-built WASI linker WRITES a file
      (c) a snippet calling open() never becomes a WASM artifact (refused at compile/build).
    The record's `contained` is True only if every positive AND every RED half held."""
    from pythscribe.build import BuildError, build_module
    from pythscribe.runtime import FuelExhausted, Sandbox, SandboxViolation, ServerKernel

    work = Path(workdir) if workdir else Path(tempfile.mkdtemp(prefix="pythscribe_sandbox_"))
    work.mkdir(parents=True, exist_ok=True)
    rec: dict[str, Any] = {"kind": "sandbox", "fuel": fuel, "workdir": str(work)}

    # positive: the "LLM snippet" compiled and run under fuel + no I/O
    snip = work / "llm_snippet.py"
    snip.write_text(LLM_SNIPPET, encoding="utf-8")
    [art] = build_module(snip, quiet=True)
    k = ServerKernel.from_artifact(art, sandbox=Sandbox(fuel=fuel))
    ids = [(i * 7919) % 5000 for i in range(20_000)]
    weights = [3, 1, 4, 1, 5, 9, 2, 6]
    r = k.call("score_tokens", ["list[int]", "list[int]", "int"], [ids, weights, 2500], return_type="int")
    mod = load_kernels(snip, "llm_snippet_for_measure")
    py = binding(mod.score_tokens).run_python(ids, weights, 2500)
    rec["snippet"] = {"value": r.value, "cpython_value": py, "equal": r.value == py, "fuel_used": r.fuel_used,
                      "host_imports": list(k.host_imports), "wasm_bytes": art.wasm.stat().st_size}

    # (a) fuel trap
    sb = binding(K.spin)
    ks = ServerKernel.from_artifact(sb.artifact, sandbox=Sandbox(fuel=fuel))
    t0 = time.perf_counter()
    try:
        ks.call("spin", ["int"], [1])
        trapped, err = False, "returned"
    except FuelExhausted as e:
        trapped, err = True, str(e)
    rec["fuel_trap"] = {"trapped": trapped, "elapsed_s": time.perf_counter() - t0, "error": err,
                        "finite_call_fuel_used": ks.call("spin", ["int"], [0]).fuel_used}
    rec["fuel_trap"]["red_control"] = _spin_without_fuel_needs_external_kill(sb.artifact.wasm.read_bytes())

    # (b) I/O import refused at link
    io_wasm = wasi_io_module_bytes()
    try:
        ServerKernel.from_wasm(io_wasm, name="io_attempt")
        refused, err = False, "instantiated"
    except SandboxViolation as e:
        refused, err = True, str(e)
    wrote, content = _run_io_module_with_wasi(io_wasm, work)
    rec["io_attempt"] = {"refused": refused, "error": err,
                         "red_control": {"wrote_file_under_wasi_linker": wrote, "content": content}}

    # (c) open() in a @wasm kernel never becomes an artifact
    leak = work / "leak_snippet.py"
    leak.write_text(IO_SNIPPET, encoding="utf-8")
    try:
        build_module(leak, quiet=True)
        compile_refused, cerr = False, "built"
    except BuildError as e:
        compile_refused, cerr = True, str(e).splitlines()[0][:200]
    rec["open_in_kernel"] = {"refused_at_build": compile_refused, "error": cerr,
                             "artifact_exists": (work / "__pythscribe__" / "leak").exists()}

    rec["contained"] = sandbox_contained(rec)
    return rec


def sandbox_contained(rec: dict) -> bool:
    """THE predicate behind `contained` (one authority, shared with the tests so a knocked-out
    leg is checked against this code, not a copy -- opus m1.5 r1/S7): every positive AND every
    RED half must hold."""
    return bool(
        rec["snippet"]["equal"]
        and rec["fuel_trap"]["trapped"] and rec["fuel_trap"]["red_control"]["still_running_at_kill"]
        and rec["io_attempt"]["refused"] and rec["io_attempt"]["red_control"]["wrote_file_under_wasi_linker"]
        and rec["open_in_kernel"]["refused_at_build"] and not rec["open_in_kernel"]["artifact_exists"]
    )


# --------------------------------------------------------------------------- 3. GIL-free fan-out

def measure_fanout(K, threads: int = 4, n: int = 1200, seed: int = 0, reps: int = 2) -> dict:
    """N threads each run the kernel `reps` times. WASM: each thread has its own wasmtime
    instance and the ctypes call releases the GIL -> wall time should scale. CONTROL: the
    same threads running the CPython body -> the GIL serialises them (scaling ~1)."""
    _require_server(K.edit_distance)
    b = binding(K.edit_distance)
    a, s = random_text(n, seed), random_text(n, seed + 1)

    def task(fn):
        for _ in range(reps):
            edit_distance_call(fn, a, s)

    def wall(fn, nthreads: int) -> float:
        ts = [threading.Thread(target=task, args=(fn,)) for _ in range(nthreads)]
        t0 = time.perf_counter()
        for t in ts:
            t.start()
        for t in ts:
            t.join()
        return time.perf_counter() - t0

    wasm_single = wall(K.edit_distance, 1)
    wasm_par = wall(K.edit_distance, threads)
    py_single = wall(b.run_python, 1)
    py_par = wall(b.run_python, threads)
    return {
        "kind": "fanout", "threads": threads, "n": n, "seed": seed, "reps": reps, "cpu_count": os.cpu_count(),
        "wasm_single_s": wasm_single, "wasm_parallel_s": wasm_par, "wasm_scaling": threads * wasm_single / wasm_par,
        "cpython_single_s": py_single, "cpython_parallel_s": py_par, "cpython_scaling": threads * py_single / py_par,
        "wasm_instances": b.server.instances,
    }


# --------------------------------------------------------------------------- 4. determinism

def _viterbi_inputs(n_states: int, n_steps: int, seed: int) -> dict:
    rng = random.Random(seed)
    return {
        "logp": [rng.uniform(-3, 0) for _ in range(n_states)],
        "trans": [rng.uniform(-3, 0) for _ in range(n_states * n_states)],
        "emit": [rng.uniform(-3, 0) for _ in range(n_states * n_steps)],
        "n_states": n_states, "n_steps": n_steps,
    }


def viterbi_call(fn, inp: dict) -> tuple[float, list[int]]:
    ns, nt = inp["n_states"], inp["n_steps"]
    score = [0.0] * (ns * nt)
    back = [0] * (ns * nt)
    path = [0] * nt
    best = fn(inp["logp"], inp["trans"], inp["emit"], ns, nt, score, back, path)
    return best, path


def digest(best: float, path: list[int]) -> str:
    return hashlib.sha256(struct.pack("<d", best) + struct.pack(f"<{len(path)}q", *path)).hexdigest()


def measure_determinism(K, runs: int = 5, n_states: int = 6, n_steps: int = 400, seed: int = 0, node: bool = True) -> dict:
    """Same `.wasm`, same bytes: sha256 of (best, path) over repeated in-process runs, and
    equal to the CPython body and -- when Node is present -- to the browser shim (V8) run
    through `pythscribe.ffi.run_kernel`: three engines, one digest."""
    _require_server(K.viterbi)
    b = binding(K.viterbi)
    inp = _viterbi_inputs(n_states, n_steps, seed)
    digests = []
    for _ in range(runs):
        best, path = viterbi_call(K.viterbi, inp)
        digests.append(digest(best, path))
    py_best, py_path = viterbi_call(b.run_python, inp)
    rec: dict[str, Any] = {
        "kind": "determinism", "runs": runs, "n_states": n_states, "n_steps": n_steps, "seed": seed,
        "digests": digests, "identical_across_runs": len(set(digests)) == 1,
        "cpython_digest": digest(py_best, py_path), "host_imports": list(b.server.host_imports),
        "wasm_sha256": b.server.wasm_sha256,
    }
    rec["equal_cpython"] = rec["cpython_digest"] == digests[0]
    if node and shutil.which("node"):
        from pythscribe.ffi import run_kernel

        ns, nt = n_states, n_steps
        [r] = run_kernel(
            b.artifact.wasm, "viterbi",
            ["list[float]", "list[float]", "list[float]", "int", "int", "list[float]", "list[int]", "list[int]"],
            [{"args": [inp["logp"], inp["trans"], inp["emit"], ns, nt, [0.0] * (ns * nt), [0] * (ns * nt), [0] * nt], "read_back": [7]}],
            return_type="float",
        )
        if not r["ok"]:
            raise RuntimeError(r["error"])
        rec["node_v8_digest"] = digest(float(r["value"]), [int(x) for x in r["outs"]["7"]])
        rec["equal_node_v8"] = rec["node_v8_digest"] == digests[0]
    else:
        rec["node_v8_digest"] = None
        rec["equal_node_v8"] = None
    return rec


# --------------------------------------------------------------------------- 5. single artifact / 6. fallback

def measure_single_artifact(K, tmpdir: Path | None = None) -> dict:
    """Only the `.wasm` is needed at run time: copy the one file elsewhere, point the
    compiler lookup at a non-existent binary, and run."""
    from pythscribe.build import BuildError, find_pyths
    from pythscribe.runtime import ServerKernel

    b = binding(K.edit_distance)
    d = Path(tmpdir) if tmpdir else Path(tempfile.mkdtemp(prefix="pythscribe_single_"))
    d.mkdir(parents=True, exist_ok=True)
    only = d / "edit_distance.wasm"
    shutil.copyfile(b.artifact.wasm, only)
    old = os.environ.get("PYTHSCRIBE_PYTHS")
    os.environ["PYTHSCRIBE_PYTHS"] = str(d / "no-such-compiler")
    try:
        try:
            find_pyths()
            compiler_reachable = True
        except BuildError:
            compiler_reachable = False
        k = ServerKernel.from_wasm(only)
        a, s = "kitten", "sitting"
        r = k.call("edit_distance", ["list[int]"] * 4, [codes(a), codes(s), [0] * 8, [0] * 8], return_type="int")
    finally:
        if old is None:
            os.environ.pop("PYTHSCRIBE_PYTHS", None)
        else:
            os.environ["PYTHSCRIBE_PYTHS"] = old
    return {"kind": "single_artifact", "wasm_bytes": only.stat().st_size, "files_needed": 1,
            "compiler_reachable": compiler_reachable, "value": r.value, "expected": edit_distance_ref("kitten", "sitting"),
            "ok": (not compiler_reachable) and r.value == edit_distance_ref("kitten", "sitting")}


def measure_fallback(tmpdir: Path | None = None) -> dict:
    """Artifacts absent -> the SAME module runs as plain Python (mode 'fallback'), same answer."""
    d = Path(tmpdir) if tmpdir else Path(tempfile.mkdtemp(prefix="pythscribe_fallback_"))
    d.mkdir(parents=True, exist_ok=True)
    shutil.copyfile(KERNELS, d / "kernels.py")
    assert not (d / "__pythscribe__").exists()
    mod = load_kernels(d / "kernels.py", "wasm_use_cases_kernels_fallback")
    b = binding(mod.edit_distance)
    v = edit_distance_call(mod.edit_distance, "kitten", "sitting")
    return {"kind": "fallback", "mode": b.mode, "mode_reason": b.mode_reason, "artifact_status": b.artifact_status,
            "value": v, "expected": edit_distance_ref("kitten", "sitting"), "python_calls": b.calls(), "server_calls": b.server_runs(),
            "ok": b.mode == "fallback" and v == edit_distance_ref("kitten", "sitting") and b.calls() == 1 and b.server_runs() == 0}


# --------------------------------------------------------------------------- 7. where it loses

def measure_where_it_loses(K, n: int = 1_000_000, per_element_n: int = 20_000, seed: int = 0, repeats: int = 3) -> dict:
    """Two honest losses, measured: (1) an already-vectorised reduction -- NumPy is C already;
    (2) crossing the boundary PER ELEMENT is marshaling-dominated -- batch the call."""
    import numpy as np

    _require_server(K.sum_squares)
    _require_server(K.is_vowel)
    rng = np.random.default_rng(seed)
    xs = rng.standard_normal(n)
    xs_list = xs.tolist()
    np_s, np_v = time_best(lambda: float(np.dot(xs, xs)), repeats)
    wasm_s, wasm_v = time_best(lambda: K.sum_squares(xs_list), repeats)  # includes marshaling n floats
    py_s, py_v = time_best(lambda: binding(K.sum_squares).run_python(xs_list), 1)
    text = random_text(per_element_n, seed, "abcdefghijklmnopqrstuvwxyz")
    cs = codes(text)
    per_s, per_v = time_best(lambda: sum(K.is_vowel(c) for c in cs), 1)
    batch_s, batch_v = time_best(lambda: K.count_vowels(cs), repeats)
    return {
        "kind": "where_it_loses", "n": n, "per_element_n": per_element_n, "seed": seed,
        "vectorised": {"numpy_s": np_s, "wasm_s": wasm_s, "cpython_s": py_s, "numpy_over_wasm_x": wasm_s / np_s,
                        "wasm_over_cpython_x": py_s / wasm_s, "values_close": abs(np_v - wasm_v) <= 1e-6 * abs(np_v), "numpy_wins": np_s < wasm_s},
        "per_element": {"per_call_s": per_s, "batched_s": batch_s, "per_element_over_batched_x": per_s / batch_s,
                         "per_call_us": per_s / per_element_n * 1e6, "equal": per_v == batch_v, "batching_wins": batch_s < per_s},
    }


# --------------------------------------------------------------------------- client rows (browser)

def measure_redaction_via_browser_shim(K, seed: int = 0, n: int = 4000, min_run: int = 4) -> dict:
    """On-device redaction: `mask_digit_runs` through the BROWSER'S OWN JS shim under Node
    (V8; the exact `list_buffer.mjs` the Gradio component ships), the server path, and
    CPython -- the masked buffer is byte-identical in all three. Stated plainly: this is the
    browser CHANNEL (V8 + the component's shim), not a tab; the tab run is the isomorphic
    cell (a real Gradio app driven by Playwright)."""
    from pythscribe.ffi import run_kernel

    _require_server(K.mask_digit_runs)
    rng = random.Random(seed)
    parts = []
    for _ in range(n // 20):
        parts.append(rng.choice(["call ", "id ", "ref ", "tel ", "x "]))
        parts.append(str(rng.randrange(10 ** rng.randrange(1, 12))))
    text = "".join(parts)
    cs = codes(text)
    b = binding(K.mask_digit_runs)
    out_server = [0] * len(cs)
    masked_server = K.mask_digit_runs(cs, min_run, out_server)
    out_py = [0] * len(cs)
    masked_py = b.run_python(cs, min_run, out_py)
    rec: dict[str, Any] = {"kind": "redaction", "n": len(cs), "seed": seed, "min_run": min_run,
                           "masked_server": masked_server, "masked_cpython": masked_py,
                           "server_digest": hashlib.sha256(struct.pack(f"<{len(cs)}q", *out_server)).hexdigest(),
                           "cpython_digest": hashlib.sha256(struct.pack(f"<{len(cs)}q", *out_py)).hexdigest(),
                           "sample_before": text[:60], "sample_after": "".join(chr(c) for c in out_server[:60])}
    if shutil.which("node"):
        [r] = run_kernel(b.artifact.wasm, "mask_digit_runs", ["list[int]", "int", "list[int]"],
                         [{"args": [cs, min_run, [0] * len(cs)], "read_back": [2]}])
        if not r["ok"]:
            raise RuntimeError(r["error"])
        rec["masked_browser_shim"] = int(r["value"])
        rec["browser_shim_digest"] = hashlib.sha256(struct.pack(f"<{len(cs)}q", *[int(x) for x in r["outs"]["2"]])).hexdigest()
    else:
        rec["masked_browser_shim"] = None
        rec["browser_shim_digest"] = None
    rec["all_identical"] = rec["server_digest"] == rec["cpython_digest"] and (rec["browser_shim_digest"] in (None, rec["server_digest"]))
    # an INDEPENDENT reference for the masking rule (a regex), so the kernel is checked
    # against a spec, not only against itself on the other paths
    import re

    expected = re.sub(r"\d{%d,}" % min_run, lambda m: "*" * len(m.group()), text)
    rec["matches_regex_reference"] = "".join(chr(c) for c in out_server) == expected
    return rec


def m1_preprocessing_evidence() -> dict | None:
    """The 'preprocessing at the edge' row: the committed M1 evidence (bytes uploaded,
    client vs naive), re-read -- not restated."""
    import pythscribe

    candidates = [Path(os.environ["PYTHSCRIBE_M1_EVIDENCE"])] if os.environ.get("PYTHSCRIBE_M1_EVIDENCE") else []
    candidates += [HERE.parent / "gradio-image-preprocess" / "metrics_summary.json",  # a checkout
                   Path(pythscribe.__file__).resolve().parents[1] / "examples" / "gradio-image-preprocess" / "metrics_summary.json"]  # a copy of this dir, editable install
    p = next((c for c in candidates if c.is_file()), None)
    if p is None:
        return None
    s = json.loads(p.read_text(encoding="utf-8"))
    return {"kind": "m1_preprocessing", "aggregate_reduction_x": s["aggregate"]["reduction_x"],
            "images": [r["image"] for r in s["rows"]], "client_paths": s["aggregate"].get("client_paths"),
            "source": "examples/gradio-image-preprocess/metrics_summary.json"}


# --------------------------------------------------------------------------- the table

def summarize(records: dict[str, Any]) -> dict:
    """The use-case table, every cell a function of the records. Raises on a missing or
    unmeasured record (never silently 'n/a' for a row the plan claims)."""
    rows: list[dict[str, Any]] = []

    def need(k: str) -> dict:
        r = records.get(k)
        if not isinstance(r, dict):
            raise ValueError(f"record {k!r} missing: the notebook must measure it")
        return r

    sp = need("speedup")
    rows.append({"row": "Non-vectorizable loops (server)", "claim": "same answer on every path; speedup vs interpreted CPython REPORTED",
                 "measured": f"{sp['ratio']:.1f}x vs CPython on {sp['n']}x{sp['n']} edit distance ({sp['wasm_s']*1e3:.1f} ms vs {sp['cpython_s']*1e3:.0f} ms); all paths == reference {sp['all_equal']}"
                             + ("" if sp["ratio"] > 1.0 else " -- NO speedup on this machine (contended/throttled?)"),
                 # correctness is the gate; the ratio is a measurement, not a pass condition (codex m1.5 r2/#6)
                 "pass": bool(sp["all_equal"])})
    sb = need("sandbox")
    rows.append({"row": "Sandboxed (LLM-generated) code (server)", "claim": "fuel-bounded, no ambient I/O; escapes are RED",
                 "measured": f"snippet ok (fuel used {sb['snippet']['fuel_used']}); loop trapped in {sb['fuel_trap']['elapsed_s']*1e3:.0f} ms; "
                             f"WASI import refused; open() refused at build; RED halves: unmetered loop ran until killed={sb['fuel_trap']['red_control']['still_running_at_kill']}, "
                             f"WASI linker wrote file={sb['io_attempt']['red_control']['wrote_file_under_wasi_linker']}",
                 "pass": bool(sb["contained"])})
    fo = need("fanout")
    cpus = fo.get("cpu_count") or 0
    rows.append({"row": "GIL-free parallelism (server)", "claim": "N threads scale; the CPython control does not",
                 "measured": (f"{fo['threads']} threads on {cpus} CPUs: WASM scaling {fo['wasm_scaling']:.2f}x, CPython control {fo['cpython_scaling']:.2f}x"
                              + ("" if cpus >= fo["threads"] else f" -- NOT MEASURABLE: fewer CPUs ({cpus}) than threads ({fo['threads']})")),
                 # an environmental limit is reported as unmeasurable, never as a failed claim (opus m1.5 r1/S8)
                 "pass": (bool(fo["wasm_scaling"] > 1.5 and fo["wasm_scaling"] > fo["cpython_scaling"] * 1.3) if cpus >= fo["threads"] else None)})
    de = need("determinism")
    v8_ran = de["equal_node_v8"] is not None
    rows.append({"row": "Bit-for-bit determinism (server)", "claim": "same bytes every run, == CPython, == V8 shim",
                 "measured": (f"{de['runs']} runs, {len(set(de['digests']))} digest(s); == CPython {de['equal_cpython']}; == Node/V8 {de['equal_node_v8']}; host imports {de['host_imports'] or 'none'}"
                              + ("" if v8_ran else " -- NOT RUN: the V8-shim arm needs node")),
                 # the V8 arm is part of the claim: absent -> NOT RUN, never a pass (opus m1.5 r1/S5)
                 "pass": (bool(de["identical_across_runs"] and de["equal_cpython"] and de["equal_node_v8"]) if v8_ran else None)})
    sa = need("single_artifact")
    rows.append({"row": "Single-artifact deployment (server)", "claim": "one .wasm, no toolchain at run time",
                 "measured": f"{sa['wasm_bytes']} B .wasm ran with the compiler unreachable ({sa['value']} == {sa['expected']})", "pass": bool(sa["ok"])})
    iso = records.get("isomorphic")
    if isinstance(iso, dict):
        # the in-process arm must have RUN (mode server, bits present): a two-way agreement is not
        # the isomorphic claim (opus m1.5 r1/B3)
        server_arm = iso.get("mode") == "server" and bool(iso.get("server_bits"))
        rows.append({"row": "Isomorphic same-fn (browser + server)", "claim": "one @wasm fn, tab and in-process, identical bits",
                     "measured": f"browser bits {iso['browser_bits']} == server bits {iso['server_bits']} == CPython {iso['cpython_bits']}: {iso['identical']} (mode={iso.get('mode')}; browser python_calls={iso['python_calls']}, server_calls={iso['server_calls']})",
                     "pass": bool(server_arm and iso["identical"] and iso["browser_bits"] == iso["server_bits"] == iso["cpython_bits"]
                                  and iso["path"] == "browser-wasm" and iso["python_calls"] == 0 and iso["server_calls"] == 0)})
        rows.append({"row": "Zero-round-trip interactivity (browser)", "claim": "the tab computes; the server does nothing",
                     "measured": f"path={iso['path']}, server python_calls={iso['python_calls']}, server_calls={iso['server_calls']}, .wasm fetched by the tab={iso['wasm_fetched']}",
                     "pass": bool(iso["path"] == "browser-wasm" and iso["wasm_fetched"] and iso["python_calls"] == 0 and iso["server_calls"] == 0)})
    else:
        rows.append({"row": "Isomorphic same-fn (browser + server)", "claim": "one @wasm fn, tab and in-process, identical bits",
                     "measured": "NOT RUN in this session (needs gradio + playwright)", "pass": None})
        rows.append({"row": "Zero-round-trip interactivity (browser)", "claim": "the tab computes; the server does nothing",
                     "measured": "NOT RUN in this session (needs gradio + playwright)", "pass": None})
    m1 = records.get("m1_preprocessing")
    rows.append({"row": "Preprocessing at the edge (browser)", "claim": "resize in the tab before upload",
                 "measured": (f"{m1['aggregate_reduction_x']:.0f}x fewer bytes uploaded across {len(m1['images'])} images (M1 evidence, {m1['source']})" if m1 else "M1 evidence not found"),
                 "pass": bool(m1 and m1["aggregate_reduction_x"] > 5) if m1 else None})
    rd = need("redaction")
    shim_ran = rd.get("browser_shim_digest") is not None
    rows.append({"row": "On-device redaction (browser channel)", "claim": "PII masked before upload; identical everywhere; == regex spec",
                 "measured": (f"{rd['masked_server']} of {rd['n']} chars masked; V8 shim / server / CPython digests identical={rd['all_identical']}; == regex reference {rd.get('matches_regex_reference')}"
                              + ("" if shim_ran else " -- NOT RUN: the V8-shim arm needs node")),
                 # the independent regex spec is part of the pass (three engines agreeing on a wrong answer must not pass -- opus m1.5 r1/S6)
                 "pass": (bool(rd["all_identical"] and rd.get("matches_regex_reference") and rd["masked_server"] == rd["masked_cpython"] == rd["masked_browser_shim"] and rd["masked_server"] > 0)
                          if shim_ran else None)})
    fb = need("fallback")
    rows.append({"row": "Fallback (no artifact)", "claim": "plain Python, same answer",
                 "measured": f"mode={fb['mode']} ({fb['artifact_status']}); value {fb['value']} == {fb['expected']}; python_calls={fb['python_calls']}", "pass": bool(fb["ok"])})
    wl = need("where_it_loses")
    v, pe = wl["vectorised"], wl["per_element"]
    vs_py = (f"@wasm {v['wasm_over_cpython_x']:.1f}x faster than CPython" if v["wasm_over_cpython_x"] >= 1.0
             else f"@wasm even {1 / v['wasm_over_cpython_x']:.0f}x SLOWER than CPython here -- marshaling {wl['n']} floats across the boundary dominates")
    rows.append({"row": "WHERE IT LOSES: already-vectorised code", "claim": "NumPy (C) is not beaten -- say so (and the answers agree)",
                 "measured": f"sum of squares, n={wl['n']}: NumPy {v['numpy_s']*1e3:.2f} ms vs @wasm {v['wasm_s']*1e3:.1f} ms vs CPython {v['cpython_s']*1e3:.0f} ms (NumPy {v['numpy_over_wasm_x']:.0f}x faster; {vs_py}); values agree {v['values_close']}",
                 "pass": bool(v["numpy_wins"] and v["values_close"])})
    rows.append({"row": "WHERE IT LOSES: per-element boundary crossing", "claim": "marshaling dominates -- batch the call",
                 "measured": f"{wl['per_element_n']} per-element calls {pe['per_call_s']*1e3:.0f} ms ({pe['per_call_us']:.1f} us/call) vs one batched call {pe['batched_s']*1e3:.2f} ms ({pe['per_element_over_batched_x']:.0f}x)",
                 "pass": bool(pe["batching_wins"] and pe["equal"])})
    counted = [r for r in rows if r["pass"] is not None]
    return {"rows": rows, "all_pass": all(r["pass"] for r in counted), "n_rows": len(rows), "n_measured": len(counted),
            "inputs": {k: {kk: vv for kk, vv in r.items() if kk in ("n", "seed", "threads", "runs", "fuel", "n_states", "n_steps", "per_element_n", "reps")}
                       for k, r in records.items() if isinstance(r, dict)}}


def format_table(summary: dict) -> str:
    lines = ["| Use case | Claim | Measured | Pass |", "|---|---|---|---|"]
    for r in summary["rows"]:
        p = "YES" if r["pass"] else ("NOT RUN" if r["pass"] is None else "**NO**")
        lines.append(f"| {r['row']} | {r['claim']} | {r['measured']} | {p} |")
    lines.append("")
    lines.append(f"{summary['n_measured']}/{summary['n_rows']} rows measured; all measured rows pass: **{summary['all_pass']}** (numbers computed, not constants)")
    return "\n".join(lines)


def save_evidence(records: dict, summary: dict, records_path: Path = RECORDS_JSON, summary_path: Path = SUMMARY_JSON) -> None:
    for p, obj in ((records_path, records), (summary_path, summary)):
        p.write_text(json.dumps(obj, indent=2, sort_keys=True) + "\n", encoding="utf-8", newline="\n")
