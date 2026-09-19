"""0.2.9: the per-kernel `@wasm`-vs-plain-Python SPEEDUP gate in demos/wasm_features_demo.ipynb.

The notebook carries a benchmark of record (`BENCHMARK_SPEEDUPS`, median-of-100 speedups for its 7 kernels,
every kernel on the server path) and a gate: a kernel below HALF its benchmark (2x headroom for a slower
host) or below 1.0x fails. HARD by default (AssertionError -> a local run STOPS); SOFT under
PYTHSCRIBE_PERF_MODE=soft (a WARNING per kernel, never an exception) -- how CI / shared runners run it, so
timing never reds a shared runner (byte-reproducible wheels: the code is identical, the host is not).

These tests exec the SHIPPED `# perf-gate-definitions` cell straight out of the .ipynb (a SPOT through
the shipped path, never a copy) and drive it both ways. Paired controls:
  G1  HARD: a synthetic kernel below threshold RAISES AssertionError naming it (measured vs threshold);
      a kernel at 0.9x raises even when its benchmark is tiny (the 1.0x floor); exactly-at-threshold passes.
  G2  SOFT (env or explicit): the SAME inputs only WARN (a printed WARNING + a UserWarning per kernel),
      never raise; a clean run warns nothing in either mode.
  G3  the 7 BENCHMARK_SPEEDUPS keys == the 7 notebook kernels (the apply cell's KERNEL_FNS), every
      benchmark > 1.0; a missing/extra kernel is refused (KeyError), an unknown mode is refused.
  N   (gated: numba/pandas/matplotlib/wasmtime/nbclient/nbformat/ipykernel + the pinned compiler) the
      notebook executes end-to-end under PYTHSCRIBE_PERF_MODE=soft from a clean copy: all 7 kernels
      mode='server', the gate line printed; and a copy whose benchmark is inflated x10 still COMPLETES in
      soft mode with exactly 7 WARNINGs (the anti-vacuity twin: soft never raises at notebook level).
"""
from __future__ import annotations

import json
import math
import os
import re
import shutil
import warnings
from pathlib import Path

import pytest

from conftest import REPO, gate, gate_import

NB = REPO / "demos" / "wasm_features_demo.ipynb"
KERNELS = ("edit_distance", "pairwise", "mandelbrot", "crc32", "viterbi", "kmeans_inertia", "hashed_energy")


def _cells() -> list[dict]:
    return json.loads(NB.read_text(encoding="utf-8"))["cells"]


def _cell_source(marker: str) -> str:
    [c] = [c for c in _cells() if c["cell_type"] == "code" and "".join(c["source"]).startswith(marker)]
    return "".join(c["source"])


def _gate_ns() -> dict:
    ns: dict = {}
    exec(compile(_cell_source("# perf-gate-definitions"), str(NB), "exec"), ns)  # the SHIPPED cell
    return ns


@pytest.fixture
def g(monkeypatch):
    monkeypatch.delenv("PYTHSCRIBE_PERF_MODE", raising=False)  # a plain local run: HARD
    return _gate_ns()


# ------------------------------------------------------------------ G3: the benchmark of record


def test_g3_benchmark_names_the_seven_notebook_kernels(g):
    bench = g["BENCHMARK_SPEEDUPS"]
    assert set(bench) == set(KERNELS) and len(bench) == 7
    apply_src = _cell_source("# perf-gate-apply")
    m = re.search(r"KERNEL_FNS = \[(.*?)\]", apply_src)
    assert m and [x.strip() for x in m.group(1).split(",")] == list(KERNELS)
    assert all(isinstance(v, (int, float)) and v > 1.0 for v in bench.values()), bench  # a recorded SPEEDUP, never a placeholder
    assert g["SPEEDUP_TOLERANCE"] == 2.0 and g["PERF_MODE_ENV"] == "PYTHSCRIBE_PERF_MODE"


def _committed_speed_table_medians() -> dict[str, tuple[float, float]]:
    """(plain-Python median, @wasm median) per kernel, parsed from the COMMITTED speed-table cell's stored
    text/plain output (the DataFrame print) -- the provenance the benchmark of record must derive from."""
    [c] = [c for c in _cells() if c["cell_type"] == "code" and "STATS = {}" in "".join(c["source"])]
    txt = "".join("".join(o.get("data", {}).get("text/plain", "")) for o in c.get("outputs", []))
    blocks = [b for b in txt.split("\n\n") if b.strip()]
    cols: dict[str, list[float]] = {}
    for b in blocks:
        head, *rows = b.splitlines()
        for col in ("plain Python (ms)", "@wasm (ms)"):
            if col in head:
                vals = [float(re.search(r"(\d+\.\d+) \[", r).group(1)) for r in rows if re.search(r"(\d+\.\d+) \[", r)]
                cols[col] = vals
    assert len(cols["plain Python (ms)"]) == 7 == len(cols["@wasm (ms)"]), cols
    return {k: (p, w) for k, p, w in zip(KERNELS, cols["plain Python (ms)"], cols["@wasm (ms)"])}


def test_g3_benchmark_is_provenance_consistent_with_the_committed_speed_table(g):
    """codex 0.2.9 r4/2e: BENCHMARK_SPEEDUPS == floor(plain median / @wasm median, 0.1x) of the SAME committed
    run whose table is stored in the notebook (a benchmark typed in, or from a different run, is RED)."""
    bench = g["BENCHMARK_SPEEDUPS"]
    meds = _committed_speed_table_medians()
    for k, (p, w) in meds.items():
        # the table PRINTS medians at 2 decimals (+-0.005 each), the benchmark was floored from the unrounded medians
        # of the same run: the true ratio lies in [r_lo, r_hi], so floor(r_lo) <= bench <= r_hi (a 0.1x-wide window
        # -- a value from another run or typed in, e.g. the old 16.4x mandelbrot vs [19.0, 19.2], is still RED)
        r_lo, r_hi = (p - 0.005) / (w + 0.005), (p + 0.005) / (w - 0.005)
        assert math.floor(r_lo * 10) / 10 - 1e-9 <= bench[k] <= r_hi + 1e-9, (k, p, w, r_lo, r_hi, bench[k])
        assert r_hi - r_lo < 0.6, (k, "print precision too coarse to bind provenance")
    # codex r5 2(c): the committed `# perf-gate-apply` output comes from the SAME execution as the table -- its
    # `this run (x)` column is the table's own ratio (to print precision) and its benchmark column is the constant
    [ac] = [c for c in _cells() if c["cell_type"] == "code" and "".join(c["source"]).startswith("# perf-gate-apply")]
    apply_out = "".join("".join(o.get("text", "")) for o in ac.get("outputs", []) if o.get("output_type") == "stream")
    assert "speedup gate: PASS" in apply_out, apply_out
    rows = {m.group(1): (float(m.group(2)), float(m.group(3))) for m in re.finditer(r"^(\w+)\s+([\d.]+)\s+[\d.]+\s+([\d.]+)\s+True\s*$", apply_out, re.M)}
    assert set(rows) == set(KERNELS), (rows, apply_out)
    for k, (p, w) in meds.items():
        shown_bench, shown_run = rows[k]
        assert shown_bench == pytest.approx(bench[k], abs=0.051), (k, shown_bench, bench[k])
        r_lo, r_hi = (p - 0.005) / (w + 0.005), (p + 0.005) / (w - 0.005)
        assert r_lo - 0.006 <= shown_run <= r_hi + 0.006, (k, shown_run, r_lo, r_hi, "apply output is from a different run than the table")


def test_g3_mismatched_kernel_sets_and_unknown_mode_are_refused(g):
    bench = g["BENCHMARK_SPEEDUPS"]
    ok = dict(bench)
    with pytest.raises(KeyError, match="different kernels"):
        g["check_speedups"]({k: v for k, v in ok.items() if k != "crc32"})
    with pytest.raises(KeyError, match="different kernels"):
        g["check_speedups"]({**ok, "extra_kernel": 5.0})
    with pytest.raises(ValueError, match="expected \"hard\" or \"soft\""):
        g["check_speedups"](ok, mode="advisory")


# ------------------------------------------------------------------ G1: HARD raises


def test_g1_hard_mode_raises_naming_the_offending_kernels(g, monkeypatch):
    bench = g["BENCHMARK_SPEEDUPS"]
    low = dict(bench)
    low["mandelbrot"] = bench["mandelbrot"] / 2 - 0.01          # just under threshold
    low["hashed_energy"] = 0.5                                   # not even beating Python
    with pytest.raises(AssertionError) as ei:
        g["check_speedups"](low)                                 # env unset -> hard
    msg = str(ei.value)
    assert "HARD mode" in msg and "mandelbrot: measured" in msg and "hashed_energy: measured" in msg
    assert f"threshold {bench['mandelbrot'] / 2:.2f}x" in msg and "edit_distance" not in msg
    with pytest.raises(AssertionError):
        g["check_speedups"](low, mode="hard")
    monkeypatch.setenv("PYTHSCRIBE_PERF_MODE", "HARD")           # case-insensitive
    with pytest.raises(AssertionError):
        g["check_speedups"](low)


def test_g1_the_one_x_floor_and_the_exact_threshold(g):
    bench = g["BENCHMARK_SPEEDUPS"]
    tiny = {k: 1.2 for k in bench}                               # benchmark 1.2 -> threshold 0.6, but 0.9x is < 1.0
    with pytest.raises(AssertionError, match="not even faster than plain Python"):
        g["check_speedups"]({**{k: 5.0 for k in bench}, "crc32": 0.9}, benchmark=tiny)
    at = {k: v / 2 for k, v in bench.items()}                    # exactly at threshold passes (>=)
    assert g["check_speedups"](at) == []
    with pytest.raises(AssertionError):                          # NaN never passes
        g["check_speedups"]({**dict(bench), "viterbi": float("nan")})
    assert g["speedup_failures"](dict(bench)) == []


# ------------------------------------------------------------------ G2: SOFT only warns


def test_g2_soft_mode_warns_and_never_raises(g, monkeypatch, capsys):
    bench = g["BENCHMARK_SPEEDUPS"]
    low = {**dict(bench), "mandelbrot": bench["mandelbrot"] / 4, "crc32": 0.7}
    monkeypatch.setenv("PYTHSCRIBE_PERF_MODE", "soft")
    with warnings.catch_warnings(record=True) as caught:
        warnings.simplefilter("always")
        fails = g["check_speedups"](low)                         # env -> soft
    assert sorted(f[0] for f in fails) == ["crc32", "mandelbrot"]
    out = capsys.readouterr().out
    assert out.count("WARNING:") == 2 and "PYTHSCRIBE_PERF_MODE=soft" in out and "mandelbrot: measured" in out
    assert [str(w.message) for w in caught if issubclass(w.category, UserWarning) and "speedup gate" in str(w.message)].__len__() == 2
    # explicit mode='soft' with the env unset -- same
    monkeypatch.delenv("PYTHSCRIBE_PERF_MODE")
    with warnings.catch_warnings(record=True) as caught:
        warnings.simplefilter("always")
        assert len(g["check_speedups"](low, mode="soft")) == 2
    assert capsys.readouterr().out.count("WARNING:") == 2
    # a clean run warns nothing in either mode
    with warnings.catch_warnings(record=True) as caught:
        warnings.simplefilter("always")
        assert g["check_speedups"](dict(bench), mode="soft") == [] and g["check_speedups"](dict(bench), mode="hard") == []
    assert capsys.readouterr().out == "" and not [w for w in caught if "speedup gate" in str(w.message)]


def test_g2_soft_mode_warns_exactly_the_seven_kernels_on_synthetic_measurements(g, monkeypatch, capsys):
    """codex r5 2(a): the EXACT warned set is asserted on SYNTHETIC measurements (every kernel at 0.4x its benchmark
    -> below benchmark/2 on ANY host, deterministically), never on real notebook timing."""
    bench = g["BENCHMARK_SPEEDUPS"]
    monkeypatch.setenv("PYTHSCRIBE_PERF_MODE", "soft")
    for _ in range(3):  # stable across repeats: pure arithmetic, no timing
        fails = g["check_speedups"]({k: v * 0.4 for k, v in bench.items()})
        assert sorted(f[0] for f in fails) == sorted(KERNELS)
        out = capsys.readouterr().out
        assert sorted(re.findall(r"WARNING: .*speedup gate: (\w+): measured", out)) == sorted(KERNELS)
    # and the complementary exact set: only the kernels pushed under the line warn
    fails = g["check_speedups"]({**dict(bench), "crc32": bench["crc32"] * 0.4, "viterbi": 0.99})
    assert sorted(f[0] for f in fails) == ["crc32", "viterbi"]


def test_g2_soft_mode_never_raises_even_under_warnings_as_errors(g, monkeypatch, capsys, caplog):
    """codex 0.2.9 r4/2b: a caller with `warnings.simplefilter('error')` turns warnings.warn into an exception;
    SOFT must still not raise -- the printed WARNING and the `pythscribe.perf` log record are the signals."""
    import logging

    bench = g["BENCHMARK_SPEEDUPS"]
    low = {**dict(bench), "viterbi": 0.3}
    monkeypatch.setenv("PYTHSCRIBE_PERF_MODE", "soft")
    caplog.set_level(logging.WARNING, logger="pythscribe.perf")
    with warnings.catch_warnings():
        warnings.simplefilter("error")
        fails = g["check_speedups"](low)                         # must NOT raise
        with pytest.raises(UserWarning):                         # the filter really is 'error' here (never vacuous)
            warnings.warn("probe", UserWarning)
    assert [f[0] for f in fails] == ["viterbi"]
    assert capsys.readouterr().out.count("WARNING:") == 1
    assert [r for r in caplog.records if r.name == "pythscribe.perf" and "viterbi: measured 0.30x" in r.getMessage()]
    # and HARD under the same filter still raises the AssertionError (not a converted warning)
    with warnings.catch_warnings():
        warnings.simplefilter("error")
        with pytest.raises(AssertionError, match="HARD mode"):
            g["check_speedups"](low, mode="hard")


# ------------------------------------------------------------------ N: the notebook, end to end (soft)


def _run_demo(tmp_path: Path, env_extra: dict[str, str], inflate: float | None = None) -> tuple[str, str]:
    for m in ("numba", "pandas", "matplotlib", "wasmtime", "ipykernel"):
        gate_import(m)
    nbformat = gate_import("nbformat")
    nbclient = gate_import("nbclient")
    from pythscribe._pin import COMPILER_VERSION
    from pythscribe.build import BuildError, find_pyths, pyths_version
    try:
        v = pyths_version(find_pyths())
    except BuildError as e:
        gate(False, f"the pinned compiler is required (compile-on-first-call): {e}")
    gate(v == COMPILER_VERSION, f"`pyths` reports {v!r} but the pin is {COMPILER_VERSION!r}")
    work = tmp_path / "demo"
    work.mkdir()
    shutil.copy(NB, work / NB.name)
    nb = nbformat.read(work / NB.name, as_version=4)
    if inflate is not None:
        [c] = [c for c in nb.cells if c.cell_type == "code" and c.source.startswith("# perf-gate-definitions")]
        before = dict(re.findall(r"'(\w+)': ([0-9.]+),", c.source))
        c.source = re.sub(r"'(\w+)': ([0-9.]+),", lambda m: f"'{m.group(1)}': {float(m.group(2)) * inflate:g},", c.source)
        after = dict(re.findall(r"'(\w+)': ([0-9.]+),", c.source))
        assert set(before) == set(KERNELS) and all(float(after[k]) == float(before[k]) * inflate for k in KERNELS), (before, after)
    saved = dict(os.environ)
    for k in ("PYTHSCRIBE_NO_JIT", "PYTHSCRIBE_MODE", "PYTHSCRIBE_PERF_MODE"):
        os.environ.pop(k, None)
    os.environ["PYTHONUTF8"] = "1"
    os.environ.update(env_extra)
    try:
        nbclient.NotebookClient(nb, timeout=1200, kernel_name="python3", resources={"metadata": {"path": str(work)}}).execute()
    finally:
        os.environ.clear()
        os.environ.update(saved)
    text = "\n".join("".join(o.get("text", "")) for c in nb.cells if c.cell_type == "code" for o in c.get("outputs", []) if o.get("output_type") == "stream")
    [apply_cell] = [c for c in nb.cells if c.cell_type == "code" and c.source.startswith("# perf-gate-apply")]
    apply_out = "".join("".join(o.get("text", "")) for o in apply_cell.get("outputs", []) if o.get("output_type") == "stream")
    return text, apply_out


def test_n_demo_notebook_executes_in_soft_mode_all_kernels_server(tmp_path):
    text, apply_out = _run_demo(tmp_path, {"PYTHSCRIBE_PERF_MODE": "soft"})
    modes = next(l for l in text.splitlines() if l.startswith("modes:"))
    assert modes.count("'server'") == 7 and "'browser'" not in modes and "'fallback'" not in modes, modes
    assert "speedup gate -- mode: soft" in apply_out and "speedup gate:" in apply_out, apply_out
    meas = json.loads(next(l for l in apply_out.splitlines() if l.startswith("MEASURED_SPEEDUPS_JSON:")).split(":", 1)[1])
    assert set(meas) == set(KERNELS) and all(math.isfinite(v) for v in meas.values())


def test_n_demo_notebook_soft_mode_completes_on_an_inflated_benchmark(tmp_path):
    """Anti-vacuity twin at notebook level: with the benchmark inflated x1000 under PYTHSCRIBE_PERF_MODE=soft the
    notebook COMPLETES (soft never raises; a hard-mode notebook would stop at the gate) and the cells after the gate
    ran. NO exact warning count is asserted from REAL timing (codex r5 2(a): host-independence cannot be bought
    with a bigger constant) -- only the host-independent self-consistency that the WARNINGs printed are exactly the
    kernels the table marked `False`; the exact-7 set is asserted on SYNTHETIC measurements in
    test_g2_soft_mode_warns_exactly_the_seven_kernels_on_synthetic_measurements."""
    text, apply_out = _run_demo(tmp_path, {"PYTHSCRIBE_PERF_MODE": "soft"}, inflate=1000.0)
    assert "speedup gate -- mode: soft" in apply_out and "speedup gate:" in apply_out, apply_out
    warned = sorted(re.findall(r"WARNING: .*speedup gate: (\w+): measured", apply_out))
    failed_rows = sorted(m.group(1) for m in re.finditer(r"^(\w+)\s+[\d.]+\s+[\d.]+\s+[\d.]+\s+False\s*$", apply_out, re.M))
    assert warned == failed_rows, (warned, failed_rows, apply_out)
    assert "identical bits: True" in text  # the cells AFTER the gate still ran
