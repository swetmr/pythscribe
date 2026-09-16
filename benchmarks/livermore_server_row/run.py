"""Livermore SERVER-row timing harness (v0.2.5 M4).

Runs each of the 24 Livermore Fortran Kernels (`tests/differential/livermore/*.ps`) FOUR ways in
one CPython process and reports a per-kernel timing table:

    CPython   — the pure-Python function (the semantic ground truth)
    NumPy     — a faithful vectorised form WHERE ONE EXISTS (else `n/a — loop-carried`)
    Numba     — `@njit` of the pure-Python function (else `rejected: <reason>` — the admission story)
    @wasm     — the SAME source compiled `--target wasm` and run IN-PROCESS under wasmtime
                (`ServerKernel`), the honest in-process competitor to Numba

This is the SERVER row of the AOT-vs-JIT 3×2 grid (`v0.2.5_plan.md` §15). The BROWSER row
(Pyodide+NumPy / Numba-wasm / @wasm) and the full published grid are a separate launch /
Paper-C-v2 deliverable — deliberately NOT here.

POSITIONING (honest, never softened): `@wasm` does NOT beat NumPy/Numba on vectorised numerics.
On the kernels NumPy can express it is the fastest column; Numba-native is typically fastest of
all on the nopython-eligible ones. `@wasm`'s edge is elsewhere — ADMISSION (it runs a kernel that
`raise`s a built-in exception caught by `except <SpecificClass>:`, which Numba's nopython frontend
rejects — NOT try/except/raise in general, which Numba accepts), determinism, the capability
sandbox, GIL-free fan-out, and a single deployable `.wasm` artifact. This harness measures the speed
row so the loss is stated with numbers, not hidden — on Livermore, ADMISSION wins is 0.

PAIRED ANTI-VACUITY CONTROL (feedback_anti_vacuity_paired_control): every @wasm row asserts the
kernel actually LANDED on WASM (the compiled module exports it). A kernel that silently stayed on
the JS path (no WASM export) is reported as `NOT-ON-WASM` — a RED cell — never as a WASM time.
`compile_and_load(..., require_wasm=True)` raises `NotOnWasm` in that case; the harness self-test
(`tests/pythscribe/test_m4_livermore_harness.py`) drives that control with a JS-only compile.

Usage:
    python benchmarks/livermore_server_row/run.py                # all 24 kernels
    python benchmarks/livermore_server_row/run.py --quick        # k03,k06 only (smoke)
    python benchmarks/livermore_server_row/run.py --kernels k03,k11 --reps 7
"""
from __future__ import annotations

import argparse
import ast
import contextlib
import io
import math
import os
import subprocess
import sys
import time
from dataclasses import dataclass, field
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
LIVERMORE = REPO / "tests" / "differential" / "livermore"
sys.path.insert(0, str(Path(__file__).resolve().parent))
sys.path.insert(0, str(REPO))

from numpy_impls import NUMPY_IMPLS  # noqa: E402

try:
    import numba  # noqa: E402
    from numba import njit  # noqa: E402
    _NUMBA_ERROR: type[BaseException] = numba.core.errors.NumbaError
    NUMBA_OK = True
except Exception:  # noqa: BLE001 - numba absent: the Numba column is `n/a`, never `rejected`
    NUMBA_OK = False
    _NUMBA_ERROR = ()  # type: ignore[assignment]

_ANN_TO_FFI = {"int": "int", "float": "float", "bool": "bool"}


class NotOnWasm(RuntimeError):
    """The kernel did not land on the WASM fast path (it stayed JS) — the paired RED control."""


@dataclass
class Kernel:
    id: str            # file stem, e.g. "k03_inner_product"
    fn_name: str       # the def name, e.g. "k03_inner_product" (usually == id, k07 differs)
    src: str
    param_types: list[str]
    return_type: str
    args: list         # concrete driver args


@dataclass
class Row:
    kernel: Kernel
    cpython_ms: float | None = None
    numpy_ms: float | None = None
    numba_ms: float | None = None
    wasm_ms: float | None = None
    ref: object = None
    notes: list[str] = field(default_factory=list)
    numpy_note: str = ""
    numba_note: str = ""
    wasm_note: str = ""


def discover_kernels(ids: list[str] | None = None) -> list[Kernel]:
    """Parse each `.ps` kernel: the def signature + the `print(repr(NAME(args)))` driver."""
    kernels: list[Kernel] = []
    for ps in sorted(LIVERMORE.glob("k*.ps")):
        stem = ps.stem
        if ids is not None and stem not in ids:
            continue
        src = ps.read_text(encoding="utf-8")
        tree = ast.parse(src)
        defs = {s.name: s for s in tree.body if isinstance(s, ast.FunctionDef)}
        if not defs:
            continue
        fn_name = _driver_fn_name(tree, set(defs), ps)  # the kernel the driver actually calls
        fn = defs[fn_name]
        params = [_ann_ffi(a.annotation, ps) for a in fn.args.args]
        ret = _ann_ffi(fn.returns, ps)
        args = _driver_args(tree, fn_name, ps)
        kernels.append(Kernel(id=stem, fn_name=fn_name, src=src, param_types=params, return_type=ret, args=args))
    return kernels


def _driver_fn_name(tree: ast.Module, def_names: set[str], ps: Path) -> str:
    """The kernel name called by the top-level `print(repr(NAME(...)))` driver (NAME must be a
    top-level def — skips helpers like k22's `pexp`)."""
    for stmt in tree.body:
        if not isinstance(stmt, ast.Expr):
            continue
        for node in ast.walk(stmt):
            if isinstance(node, ast.Call) and isinstance(node.func, ast.Name) and node.func.id in def_names:
                return node.func.id
    raise ValueError(f"{ps.name}: no top-level driver call of a defined kernel found")


def _ann_ffi(node, ps: Path) -> str:
    if not isinstance(node, ast.Name) or node.id not in _ANN_TO_FFI:
        raise ValueError(f"{ps.name}: only scalar int/float/bool annotations are handled, got {ast.dump(node) if node else None}")
    return _ANN_TO_FFI[node.id]


def _driver_args(tree: ast.Module, fn_name: str, ps: Path) -> list:
    """Extract the literal args of the top-level `print(repr(fn_name(...)))` driver call. Only
    TOP-LEVEL statements are considered, so a recursive self-call inside the body is never mistaken
    for the driver."""
    for stmt in tree.body:
        if not isinstance(stmt, ast.Expr):
            continue
        for node in ast.walk(stmt):
            if isinstance(node, ast.Call) and isinstance(node.func, ast.Name) and node.func.id == fn_name:
                return [ast.literal_eval(a) for a in node.args]
    raise ValueError(f"{ps.name}: no top-level driver call `{fn_name}(...)` found")


def _cpython_fn(kernel: Kernel):
    """Exec the kernel source (suppressing its print driver's stdout) and return the function."""
    ns: dict = {}
    with contextlib.redirect_stdout(io.StringIO()):
        exec(compile(kernel.src, f"<{kernel.id}>", "exec"), ns)  # noqa: S102 - trusted in-repo source
    return ns[kernel.fn_name]


def _time(fn, reps: int) -> float:
    """Best (min) wall time in ms over `reps` runs — min rejects scheduler noise."""
    best = math.inf
    for _ in range(reps):
        t0 = time.perf_counter()
        fn()
        dt = (time.perf_counter() - t0) * 1e3
        best = min(best, dt)
    return best


def compile_and_load(kernel: Kernel, tmp: Path, *, require_wasm: bool = True):
    """Compile the kernel `--target wasm` and load it under wasmtime. Raises NotOnWasm if the
    kernel did not land on WASM (the paired RED control) when require_wasm is True."""
    from pythscribe.build import find_pyths
    from pythscribe.runtime import Sandbox, ServerKernel

    ps = tmp / f"{kernel.id}.ps"
    ps.write_text(kernel.src, encoding="utf-8")
    wasm = tmp / f"{kernel.id}.wasm"
    r = subprocess.run(
        [str(find_pyths()), "compile", str(ps), "--target", "wasm", "-o", str(wasm)],
        capture_output=True, text=True,
    )
    if r.returncode != 0:
        raise NotOnWasm(f"{kernel.id}: pyths compile failed: {r.stderr.strip() or r.stdout.strip()}")
    if not (wasm.is_file() and wasm.stat().st_size > 0):
        if require_wasm:
            raise NotOnWasm(f"{kernel.id}: no .wasm emitted — the kernel stayed on the JS path")
        return None
    k = ServerKernel.from_wasm(wasm, name=kernel.id, sandbox=Sandbox())
    if kernel.fn_name not in k.exports:
        if require_wasm:
            raise NotOnWasm(f"{kernel.id}: `{kernel.fn_name}` not exported — did not land on WASM (exports: {k.exports})")
        return None
    return k


def _numba_target(kernel: Kernel):
    """njit EVERY top-level def in a fresh namespace (helpers included) and return the driver
    kernel's dispatcher. Jitting the helpers is what a real Numba user does — leaving a helper as
    a plain Python function is what falsely makes a kernel look 'rejected' (e.g. k22's `pexp`). The
    first call (in run_row) triggers compilation; a nopython typing failure raises a NumbaError
    there, which run_row classifies as `rejected` — the honest admission story."""
    import types
    ns: dict = {}
    with contextlib.redirect_stdout(io.StringIO()):
        exec(compile(kernel.src, f"<{kernel.id}>", "exec"), ns)  # noqa: S102 - trusted in-repo source
    # rebind each user def to its njit dispatcher IN PLACE. Because every def's __globals__ IS `ns`,
    # an inter-function call (target -> helper) then resolves to the JITTED helper — so k22's `pexp`
    # is jitted just as a real Numba user would decorate it, not left plain-Python to look 'rejected'.
    for name, obj in list(ns.items()):
        if isinstance(obj, types.FunctionType):
            ns[name] = njit(obj)
    return ns[kernel.fn_name]


def run_row(kernel: Kernel, tmp: Path, reps: int) -> Row:
    row = Row(kernel=kernel)

    # --- CPython (ground truth) ---
    cpy = _cpython_fn(kernel)
    row.ref = cpy(*kernel.args)
    row.cpython_ms = _time(lambda: cpy(*kernel.args), reps)

    # --- @wasm (in-process wasmtime) — with the LANDED paired control ---
    try:
        k = compile_and_load(kernel, tmp, require_wasm=True)
        call = lambda: k.call(kernel.fn_name, kernel.param_types, kernel.args, return_type=kernel.return_type).value
        wasm_val = call()
        if repr(wasm_val) != repr(row.ref):
            row.wasm_note = f"MISMATCH cpython={row.ref!r} wasm={wasm_val!r}"
            row.notes.append(row.wasm_note)
        else:
            row.wasm_ms = _time(call, reps)
    except NotOnWasm as e:
        row.wasm_note = "NOT-ON-WASM"
        row.notes.append(str(e))
    except Exception as e:  # noqa: BLE001 - report, don't crash the whole run
        row.wasm_note = f"ERROR: {type(e).__name__}: {str(e).splitlines()[0][:80]}"
        row.notes.append(row.wasm_note)

    # --- Numba (@njit of the CPython fn + all its helpers) — a NumbaError rejection IS data ---
    if not NUMBA_OK:
        row.numba_note = "n/a: numba not installed"  # NOT counted as an admission win
    else:
        try:
            j = _numba_target(kernel)
            nb_val = j(*kernel.args)  # first call triggers compile (a nopython typing failure raises here)
            if not _close(nb_val, row.ref):
                row.numba_note = f"MISMATCH cpython={row.ref!r} numba={nb_val!r}"
                row.notes.append(row.numba_note)
            else:
                row.numba_ms = _time(lambda: j(*kernel.args), reps)
        except _NUMBA_ERROR as e:  # ONLY a real Numba typing/lowering rejection counts as `rejected`
            row.numba_note = f"rejected: {type(e).__name__}"
        except Exception as e:  # noqa: BLE001 - anything else is a harness ERROR that FAILS the run
            row.numba_note = f"ERROR: {type(e).__name__}: {str(e).splitlines()[0][:60]}"
            row.notes.append(row.numba_note)

    # --- NumPy (vectorised, where a faithful form exists) ---
    np_impl = NUMPY_IMPLS.get(kernel.fn_name)
    if np_impl is None:
        row.numpy_note = "n/a: loop-carried"
    else:
        np_val = np_impl(*kernel.args)
        if not _close(np_val, row.ref):
            row.numpy_note = f"MISMATCH cpython={row.ref!r} numpy={np_val!r}"
            row.notes.append(row.numpy_note)  # N2: a wrong NumPy impl is a HARD failure, like @wasm/Numba
        else:
            row.numpy_ms = _time(lambda: np_impl(*kernel.args), reps)
    return row


def _close(a, b) -> bool:
    if isinstance(a, bool) or isinstance(b, bool):
        return a == b
    if isinstance(a, int) and isinstance(b, int):
        return a == b
    try:
        return math.isclose(float(a), float(b), rel_tol=1e-9, abs_tol=1e-12)
    except (TypeError, ValueError):
        return a == b


def _cell(ms: float | None, note: str, width: int) -> str:
    if ms is not None:
        return f"{ms:{width}.3f}"
    text = note or "-"
    return f"{text[:width]:>{width}s}"


def print_table(rows: list[Row]) -> None:
    NPW, NBW, WSW = 18, 22, 9  # column widths (each holds either a time or a note)
    print()
    print("Livermore SERVER-row timings (ms, best of N reps; lower is faster)")
    print("=" * 118)
    print(f"{'kernel':22s} {'CPython':>9s} {'NumPy':>{NPW}s} {'Numba':>{NBW}s} {'@wasm':>{WSW}s}   ratios (x CPython)")
    print("-" * 118)
    wins = {"numpy": 0, "numba": 0, "wasm": 0}
    admit_wins = 0
    for r in rows:
        ratios = []
        if r.cpython_ms is not None:
            for label, ms in (("np", r.numpy_ms), ("nb", r.numba_ms), ("ws", r.wasm_ms)):
                if ms is not None:  # N5: a measured 0.0 ms is a time, not "absent"
                    ratios.append(f"{label} {r.cpython_ms / ms:5.1f}x" if ms else f"{label}  inf x")
        npy = _cell(r.numpy_ms, r.numpy_note, NPW)
        nb = _cell(r.numba_ms, r.numba_note, NBW)
        ws = _cell(r.wasm_ms, r.wasm_note, WSW)
        print(f"{r.kernel.id:22s} {r.cpython_ms:9.3f} {npy} {nb} {ws}   {'  '.join(ratios)}")
        # tallies
        times = {k: v for k, v in (("numpy", r.numpy_ms), ("numba", r.numba_ms), ("wasm", r.wasm_ms)) if v is not None}
        if times:
            wins[min(times, key=times.get)] += 1
        if r.numba_note.startswith("rejected") and r.wasm_ms is not None:
            admit_wins += 1
    print("-" * 118)
    print(f"fastest-column tally: NumPy {wins['numpy']}  Numba {wins['numba']}  @wasm {wins['wasm']}"
          f"   (of {len(rows)} kernels)")
    print(f"ADMISSION wins (@wasm ran a kernel Numba REJECTED): {admit_wins}")
    print()
    print("Honest reading: NumPy/Numba are faster on the vectorisable/nopython kernels — @wasm does")
    print("NOT beat them on vectorised numerics. @wasm's value is admission (the rejected column),")
    print("determinism, the sandbox, GIL-free fan-out, and a single deployable artifact. See README.")


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(description="Livermore server-row timing harness (v0.2.5 M4)")
    ap.add_argument("--quick", action="store_true", help="smoke: k03,k06 only")
    ap.add_argument("--kernels", type=str, default="", help="comma-separated kernel stems (e.g. k03_inner_product,k11_first_sum) or short ids (k03)")
    ap.add_argument("--reps", type=int, default=5)
    args = ap.parse_args(argv)

    ids: list[str] | None = None
    if args.quick:
        ids = ["k03_inner_product", "k06_recurrence"]
    elif args.kernels:
        want = [s.strip() for s in args.kernels.split(",") if s.strip()]
        all_ids = [p.stem for p in LIVERMORE.glob("k*.ps")]
        ids = [next((a for a in all_ids if a == w or a.startswith(w + "_")), w) for w in want]

    kernels = discover_kernels(ids)
    if not kernels:
        print("no kernels found", file=sys.stderr)
        return 1

    import tempfile
    rows: list[Row] = []
    with tempfile.TemporaryDirectory(prefix="m4_livermore_") as td:
        tmp = Path(td)
        for kernel in kernels:
            rows.append(run_row(kernel, tmp, args.reps))
    print_table(rows)
    # Hard failures for the KNOWN-WASM-eligible Livermore set: a MISMATCH (silent wrong value), an
    # unexpected ERROR, or a @wasm column that did not produce a time — i.e. the kernel did NOT land
    # on WASM (NOT-ON-WASM) or its value disagreed with CPython. Every one of the 24 Livermore
    # kernels is expected to run on the WASM/server path, so a missing @wasm time is a regression,
    # never a silent pass (this is what makes the harness's paired landed-control load-bearing —
    # see test_m4_livermore_harness.py::test_h1_harness_quick_runs_green). `rejected`/`n/a` on the
    # NumPy/Numba columns are reported, not failed (they are the honest admission story).
    bad = []
    for r in rows:
        why = None
        if any("MISMATCH" in n or n.startswith("ERROR") for n in r.notes):
            why = "; ".join(r.notes)
        elif r.wasm_ms is None:
            why = f"@wasm did not run: {r.wasm_note or 'no time'}"
        if why:
            bad.append((r.kernel.id, why))
    if bad:
        print("\nFAILURES (hard):", file=sys.stderr)
        for kid, why in bad:
            print(f"  - {kid}: {why}", file=sys.stderr)
    return 1 if bad else 0


if __name__ == "__main__":
    raise SystemExit(main())
