"""M7-8 / M7-8b -- `pyths doctor` tracks the REAL environment (spec §3.3, plan §M7.2b, validation §M7-8).

Each probe goes through the SAME resolver the commands use (`find_pyths`, `_web.find_node`,
`resolve_wasm_opt`), so doctor can never disagree with `pyths dev` about which toolchain exists. Each
line has its OWN absent-case test -- so a mutant that hardcodes "present" on any single line goes RED
in exactly that line's test (no line is vacuously green). The Node line has the discriminating
control: `find_node` (not `shutil.which`) is the authority, proven by a case where they disagree.

  M7-8   per-line present/absent tracking; --json agrees with the text; --strict --require exits
         non-zero iff a required line is absent.
  M7-8b  every `pip install pythscribe[<x>]` names a REAL declared extra; no `[optimize]` (none ships).
"""
from __future__ import annotations

import io
import json
import os
import re
import shutil
from pathlib import Path

import pytest

from conftest import gate
from pythscribe import _launcher, _web

NODE = shutil.which("node")
DECLARED_EXTRAS = {"server", "gradio", "streamlit", "all", "test", "web-bundled"}


def _by_key() -> dict[str, dict]:
    return {ln["key"]: ln for ln in _launcher.doctor_report()}


def _all_absent(monkeypatch):
    """Force EVERY optional line absent: no Node, no wasm-opt, no wasmtime/gradio/streamlit, no
    compiler (the source tree has no bundled _bin; find_pyths may still find a dev binary -- guarded
    by callers that care)."""
    monkeypatch.setenv("PATH", "")
    monkeypatch.delenv(_web.PYTHS_NODE_ENV, raising=False)
    monkeypatch.setattr(_web, "_vendored_node", lambda: None)
    monkeypatch.setattr(_launcher, "_importable", lambda m: False)
    monkeypatch.setenv("PYTHS_WASM_OPT", str(Path(os.getcwd()) / "does-not-exist-abs" / "wasm-opt-missing"))


# ----------------------------------------------------------------- Node line (with the discriminator)


def test_node_line_present_when_system_node(monkeypatch):
    gate(bool(NODE), "node required for the present arm")
    monkeypatch.delenv(_web.PYTHS_NODE_ENV, raising=False)
    ln = _by_key()["node"]
    assert ln["present"] is True and "Node v" in ln["detail"]


def test_node_line_absent_when_no_node(monkeypatch):
    monkeypatch.setenv("PATH", "")
    monkeypatch.delenv(_web.PYTHS_NODE_ENV, raising=False)
    monkeypatch.setattr(_web, "_vendored_node", lambda: None)
    ln = _by_key()["node"]
    assert ln["present"] is False and "web-bundled" in ln["fix"]


def test_node_line_uses_find_node_not_which(monkeypatch):
    """The discriminating control (validation §M7-8): with PYTHS_NODE set to a RELATIVE value and a
    real Node on PATH, `find_node` refuses (absent) while `shutil.which('node')` would still find the
    PATH Node. doctor must say ABSENT -- proving it consults `find_node`, not `which`. A mutant whose
    Node probe uses `shutil.which` reports present here -> RED."""
    gate(bool(NODE), "a real node on PATH is required so which() and find_node() can disagree")
    monkeypatch.setenv(_web.PYTHS_NODE_ENV, "./relative-node")  # find_node refuses; which finds PATH node
    assert shutil.which("node") is not None  # which WOULD find it
    ln = _by_key()["node"]
    assert ln["present"] is False  # find_node is the authority


# ----------------------------------------------------------------- wasmtime / adapters (import seam)


def test_wasmtime_line_flips_with_importability(monkeypatch):
    monkeypatch.setattr(_launcher, "_importable", lambda m: m == "wasmtime")
    assert _by_key()["wasmtime"]["present"] is True
    monkeypatch.setattr(_launcher, "_importable", lambda m: False)
    absent = _by_key()["wasmtime"]
    assert absent["present"] is False and absent["fix"] == "`pip install pythscribe[server]`"


def test_gradio_line_flips_with_importability(monkeypatch):
    monkeypatch.setattr(_launcher, "_importable", lambda m: m == "gradio")
    assert _by_key()["gradio"]["present"] is True
    monkeypatch.setattr(_launcher, "_importable", lambda m: False)
    assert _by_key()["gradio"]["present"] is False


def test_streamlit_line_flips_with_importability(monkeypatch):
    monkeypatch.setattr(_launcher, "_importable", lambda m: m == "streamlit")
    assert _by_key()["streamlit"]["present"] is True
    monkeypatch.setattr(_launcher, "_importable", lambda m: False)
    assert _by_key()["streamlit"]["present"] is False


# ----------------------------------------------------------------- WASM optimiser line


def test_optimizer_line_absent_without_wasm_opt(monkeypatch):
    monkeypatch.setenv("PYTHS_WASM_OPT", str(Path.cwd() / "abs-missing" / "wasm-opt"))
    ln = _by_key()["wasm-opt"]
    # locally wasm-opt is absent; the fix names PATH / PYTHS_WASM_OPT and NO pip extra (M7-8b)
    assert ln["present"] is False
    assert "PYTHS_WASM_OPT" in ln["fix"] and "pythscribe[" not in ln["fix"]


# ----------------------------------------------------------------- compiler line


def test_compiler_line_present_when_find_pyths_resolves(monkeypatch):
    from pythscribe import build as _pyb
    from pythscribe._pin import COMPILER_VERSION

    monkeypatch.setattr(_pyb, "find_pyths", lambda: Path("/fake/pyths"))
    monkeypatch.setattr(_pyb, "pyths_version", lambda p: COMPILER_VERSION)
    ln = _by_key()["compiler"]
    assert ln["present"] is True and COMPILER_VERSION in ln["detail"]


def test_compiler_line_absent_reports_not_bundled(monkeypatch):
    from pythscribe import build as _pyb
    from pythscribe.build import BuildError

    def _raise():
        raise BuildError("no compiler in this test")

    monkeypatch.setattr(_pyb, "find_pyths", _raise)
    ln = _by_key()["compiler"]
    assert ln["present"] is False and ln["state"] == "not bundled"


# ----------------------------------------------------------------- MCP


def test_mcp_line_is_informational_v03(monkeypatch):
    ln = _by_key()["mcp"]
    assert ln["present"] is None and "v0.3" in ln["detail"]


# ----------------------------------------------------------------- --json / --strict / M7-8b


def test_json_agrees_with_text(monkeypatch, capsys):
    gate(bool(NODE), "node present so at least one line is present in this run")
    _launcher._doctor(["--json"])
    data = json.loads(capsys.readouterr().out)
    keys = {ln["key"] for ln in data["lines"]}
    assert {"compiler", "node", "wasm-opt", "wasmtime", "gradio", "streamlit", "mcp"} <= keys
    # the node line's present flag matches a fresh probe
    node_json = next(l for l in data["lines"] if l["key"] == "node")
    assert node_json["present"] == _by_key()["node"]["present"]


def test_strict_require_node_exits_nonzero_when_absent(monkeypatch, capsys):
    monkeypatch.setenv("PATH", "")
    monkeypatch.delenv(_web.PYTHS_NODE_ENV, raising=False)
    monkeypatch.setattr(_web, "_vendored_node", lambda: None)
    rc = _launcher._doctor(["--strict", "--require", "node"])
    capsys.readouterr()
    assert rc != 0


def test_strict_require_node_exits_zero_when_present(monkeypatch, capsys):
    gate(bool(NODE), "node required for the present arm")
    monkeypatch.delenv(_web.PYTHS_NODE_ENV, raising=False)
    rc = _launcher._doctor(["--strict", "--require", "node"])
    capsys.readouterr()
    assert rc == 0


def test_strict_without_require_is_always_zero(monkeypatch, capsys):
    rc = _launcher._doctor(["--strict"])
    capsys.readouterr()
    assert rc == 0  # a report, not a gate, unless a line is named


def test_m7_8b_fix_text_names_only_real_extras(monkeypatch, capsys):
    """With every optional line absent, doctor prints every fix; each `pythscribe[<x>]` must name a
    DECLARED extra and `[optimize]` must never appear (validation §M7-8b)."""
    _all_absent(monkeypatch)
    _launcher._doctor([])
    out = capsys.readouterr().out
    assert "[optimize]" not in out
    for x in re.findall(r"pythscribe\[([a-z0-9,\-]+)\]", out):
        for part in x.split(","):
            assert part in DECLARED_EXTRAS, (part, out)
