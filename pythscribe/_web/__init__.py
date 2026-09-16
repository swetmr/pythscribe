"""M7 (spec 13-09-26-lib-selfcontained-pip-wheel): the `.ps` frontend toolchain resolvers.

The base wheel is node-free and offline; running a React/Next dev loop is irreducibly Node, so the
frontend commands (`pyths new / install / dev / node / npm`) reach the Node the MACHINE has and offer
a vendored one as the single opt-in extra `[web-bundled]`. **Capability = environment, not tier.**

ONE Node resolver -- `find_node()` -- mirrors `pythscribe.build.find_pyths`'s discipline (absolute
paths only, never the cwd; a relative override refused). Order (§M7.2):

  1. `PYTHS_NODE` env override -- an absolute executable, or a bare name PATH-searched; a RELATIVE
     value is refused (it would resolve against the cwd -- CWE-426).
  2. system Node on PATH -- absolute PATH dirs only, never the cwd; validated `node --version` >= the
     pinned major.
  3. vendored Node from the `nodejs_wheel` package (located via `importlib.resources`) IF the
     `[web-bundled]` extra is installed.

`find_npm()` / `find_npx()` resolve beside the chosen Node (as `[node, <tool>-cli.js]` so a Windows
`.cmd` shim is never spawned). The resolver PRINTS which Node it uses and a NOTICE on the vendored
fallback. When none resolves it raises the single `ToolchainMissing` carrying the three-part,
state-tailored §3.2 report; the launcher catches it AT THE BOUNDARY and prints it with no traceback
(a mutant that raises elsewhere, or lets it escape, is RED -- validation §M7-5).
"""
from __future__ import annotations

import os
import re
import subprocess
import sys
from dataclasses import dataclass
from pathlib import Path

__all__ = [
    "PINNED_NODE_MAJOR", "PYTHS_NODE_ENV", "NodeInfo", "ToolchainMissing",
    "find_node", "find_npm", "find_npx", "find_scaffolder", "SCAFFOLDER_DIRNAME",
]

# The Node major the scaffold's Next/Vite versions require (the LTS line). This MUST equal the
# `[web-bundled]` pin's major (`nodejs-wheel-binaries>=22.12,<23`) AND the scaffolder's
# `packages/create-pyths-app/package.json` `engines.node` major -- gated by validation §M7-7.
PINNED_NODE_MAJOR = 22
PYTHS_NODE_ENV = "PYTHS_NODE"

SCAFFOLDER_DIRNAME = "create-pyths-app"
_VENDORED_SCAFFOLDER = Path(__file__).resolve().parent / SCAFFOLDER_DIRNAME
_REPO_ROOT = Path(__file__).resolve().parents[2]

_NODE_VERSION_LINE = re.compile(r"^v?(\d+)\.(\d+)\.(\d+)")


class ToolchainMissing(RuntimeError):
    """The single exception the frontend commands raise when Node cannot be resolved (or the vendored
    scaffolder is damaged). It carries the full, state-tailored §3.2 report in `.report`; the launcher
    prints that at the boundary and exits non-zero -- there is no other path to that message, so a
    mutant that raises it elsewhere (or lets it escape the boundary as a traceback) is RED."""

    def __init__(self, report: str):
        super().__init__(report)
        self.report = report


@dataclass(frozen=True)
class NodeInfo:
    node: Path
    version: str  # "22.15.0"
    major: int
    source: str  # "PYTHS_NODE" | "PATH" | "vendored"


# --------------------------------------------------------------------------- path discipline (mirrors procutil / pyths-safe.js)


def _is_bare_name(s: str) -> bool:
    """A bare program name: no separator, and (on Windows) no `:` (drive-relative `C:x`, ADS `p:ads`)."""
    if "/" in s or "\\" in s:
        return False
    if os.name == "nt" and ":" in s:
        return False
    return s not in ("", ".", "..")


def _is_truly_absolute(p: str) -> bool:
    """Anchored independently of BOTH the cwd and the current drive (win rooted-driveless `\\x`
    resolves against the current drive -- refuse it)."""
    if not os.path.isabs(p):
        return False
    if os.name != "nt":
        return True
    return bool(re.match(r"^[A-Za-z]:[\\/]", p) or re.match(r"^[\\/]{2}[^\\/]+[\\/][^\\/]", p))


def _exe_names(name: str) -> list[str]:
    if os.name != "nt":
        return [name]
    pathext = [e.strip().lower() for e in os.environ.get("PATHEXT", ".EXE;.CMD;.BAT;.COM").split(os.pathsep) if e.strip()]
    if any(name.lower().endswith(e) for e in (".exe", ".cmd", ".bat", ".com")):
        return [name]
    return [name + ext for ext in pathext] + [name]


def _search_path(name: str) -> Path | None:
    """First `name` on PATH -- absolute PATH dirs ONLY, never the cwd / empty / relative element
    (CWE-426). Mirrors `procutil::search_path` and `find_pyths`'s `_path_candidates`."""
    for d in os.environ.get("PATH", "").split(os.pathsep):
        if not d or d in (".", "..") or not os.path.isabs(d):
            continue
        for cand_name in _exe_names(name):
            p = Path(d) / cand_name
            if p.is_file():
                return Path(os.path.normpath(p.resolve()))
    return None


# --------------------------------------------------------------------------- version probe


def _probe_node_version(exe: Path) -> tuple[int, str] | None:
    """`(major, "X.Y.Z")` or None (spawn/parse failure). Never raises."""
    try:
        out = subprocess.run([str(exe), "--version"], capture_output=True, text=True, check=False, timeout=30)
    except (OSError, subprocess.SubprocessError):
        return None
    if out.returncode != 0:
        return None
    m = _NODE_VERSION_LINE.match((out.stdout or "").strip())
    if not m:
        return None
    return int(m.group(1)), f"{m.group(1)}.{m.group(2)}.{m.group(3)}"


# --------------------------------------------------------------------------- vendored Node (the [web-bundled] extra)


def _vendored_node_dir() -> Path | None:
    """The `nodejs_wheel` package's bin dir (where its `node[.exe]` lives), or None when the
    `[web-bundled]` extra is not installed. Located via importlib.resources -- no PATH, no cwd."""
    try:
        import importlib.resources as ir

        import nodejs_wheel  # noqa: F401  (presence check)

        base = Path(str(ir.files("nodejs_wheel")))
    except Exception:
        return None
    node = "node.exe" if os.name == "nt" else "node"
    for cand in (base, base / "bin"):
        if (cand / node).is_file():
            return cand
    return None


def _vendored_node() -> tuple[Path, tuple[int, str] | None] | None:
    """`(node_exe, version_or_None)` when `[web-bundled]` is installed (version None == broken), else
    None. This is the seam the tests inject a fake vendored Node through."""
    d = _vendored_node_dir()
    if d is None:
        return None
    exe = d / ("node.exe" if os.name == "nt" else "node")
    if not exe.is_file():
        return None
    return exe, _probe_node_version(exe)


# --------------------------------------------------------------------------- the three-part §3.2 message


_PART1 = (
    "pyths new / pyths install / pyths dev (the .ps React/Next frontend dev loop) need Node.js."
)
_PART3 = (
    "What still works without Node: `pyths build` / `pyths compile` (JS + WASM), `@wasm` kernels "
    "(wasmtime, in-process) and the Gradio / Streamlit adapters -- none of these need Node."
)
_DOCTOR = "Then run `pyths doctor` to see what this machine can do."


def _build_report(fix: str) -> str:
    return f"pyths: {_PART1}\n  fix: {fix}\n        {_DOCTOR}\n  {_PART3}"


def _fix_no_node() -> str:
    return (
        f"no Node.js was found on PATH. Install Node (https://nodejs.org, >= {PINNED_NODE_MAJOR}), "
        "or `pip install pythscribe[web-bundled]` (vendors a Node into this environment)."
    )


def _fix_too_old(node: Path, version: str) -> str:
    return (
        f"found Node {version} at {node}; need >= {PINNED_NODE_MAJOR}: upgrade it, set "
        f"{PYTHS_NODE_ENV} to a newer one, or `pip install pythscribe[web-bundled]`."
    )


def _fix_path_broken(node: Path) -> str:
    return (
        f"Node at {node} would not report a version (`node --version` failed). Reinstall Node "
        f"(https://nodejs.org, >= {PINNED_NODE_MAJOR}), or `pip install pythscribe[web-bundled]`."
    )


def _fix_invalid_override(value: str, reason: str) -> str:
    return f"{PYTHS_NODE_ENV}={value!r} is not an executable Node (>= {PINNED_NODE_MAJOR}): {reason}."


def _fix_vendored_broken() -> str:
    return (
        "the vendored Node (pythscribe[web-bundled]) is broken: "
        "`pip install --force-reinstall nodejs-wheel-binaries`."
    )


def _fix_vendored_too_old(node: Path, version: str) -> str:
    return (
        f"the vendored Node is {version} at {node}, older than the pinned major "
        f">= {PINNED_NODE_MAJOR}: `pip install --upgrade nodejs-wheel-binaries`, "
        f"install a newer system Node, or set {PYTHS_NODE_ENV} to one."
    )


# --------------------------------------------------------------------------- the resolver


def _resolve_override(value: str) -> NodeInfo:
    if _is_bare_name(value):
        found = _search_path(value)
        if found is None:
            raise ToolchainMissing(_build_report(_fix_invalid_override(value, "not found on PATH")))
        node = found
    elif _is_truly_absolute(value):
        node = Path(os.path.normpath(value))
        if not node.is_file():
            raise ToolchainMissing(_build_report(_fix_invalid_override(value, "no such file")))
    else:
        raise ToolchainMissing(_build_report(_fix_invalid_override(
            value, "a relative or drive-relative value resolves against the current directory (CWE-426); "
                   "use an absolute path or a bare name"
        )))
    ver = _probe_node_version(node)
    if ver is None:
        raise ToolchainMissing(_build_report(_fix_invalid_override(value, "`node --version` did not run")))
    if ver[0] < PINNED_NODE_MAJOR:
        raise ToolchainMissing(_build_report(_fix_invalid_override(value, f"Node {ver[1]} is older than {PINNED_NODE_MAJOR}")))
    return NodeInfo(node, ver[1], ver[0], "PYTHS_NODE")


def find_node(*, print_notice: bool = True, out=None) -> NodeInfo:
    """Resolve Node per §M7.2, or raise `ToolchainMissing` with the state-tailored §3.2 report.
    Prints `pyths: using <source> Node vX at <path>` (and a fallback NOTICE for the vendored one) to
    `out` (default stderr, so command stdout stays clean) unless `print_notice` is False."""
    out = out if out is not None else sys.stderr

    override = os.environ.get(PYTHS_NODE_ENV)
    if override:
        info = _resolve_override(override)
        if print_notice:
            print(f"pyths: using Node v{info.version} at {info.node} (from {PYTHS_NODE_ENV})", file=out)
        return info

    # 2. system Node on PATH
    path_state: tuple[str, Path, str | None] | None = None
    path_node = _search_path("node")
    if path_node is not None:
        ver = _probe_node_version(path_node)
        if ver is None:
            path_state = ("path_broken", path_node, None)
        elif ver[0] >= PINNED_NODE_MAJOR:
            info = NodeInfo(path_node, ver[1], ver[0], "PATH")
            if print_notice:
                print(f"pyths: using system Node v{info.version} at {info.node}", file=out)
            return info
        else:
            path_state = ("too_old", path_node, ver[1])

    # 3. vendored Node ([web-bundled])
    vend = _vendored_node()
    if vend is not None:
        node, ver = vend
        if ver is None:
            raise ToolchainMissing(_build_report(_fix_vendored_broken()))
        # S8: the vendored arm must honour the SAME pinned-major floor as the system arm -- a stale
        # `nodejs-wheel-binaries` (e.g. Node 18) is NOT usable just because it is vendored, or the
        # scaffold's Next/Vite would fail on an old runtime. Refuse it loudly, never return it.
        if ver[0] < PINNED_NODE_MAJOR:
            raise ToolchainMissing(_build_report(_fix_vendored_too_old(node, ver[1])))
        info = NodeInfo(node, ver[1], ver[0], "vendored")
        if print_notice:
            print(
                f"pyths: no usable system Node found; using the vendored Node v{info.version} from "
                f"pythscribe[web-bundled] at {info.node}",
                file=out,
            )
        return info

    # none resolved -- the state-tailored §3.2 error
    if path_state is not None and path_state[0] == "too_old":
        raise ToolchainMissing(_build_report(_fix_too_old(path_state[1], path_state[2] or "?")))
    if path_state is not None and path_state[0] == "path_broken":
        raise ToolchainMissing(_build_report(_fix_path_broken(path_state[1])))
    raise ToolchainMissing(_build_report(_fix_no_node()))


# --------------------------------------------------------------------------- npm / npx beside the chosen Node


def _cli_js(node: Path, tool: str) -> Path | None:
    """`<tool>-cli.js` shipped with the Node distribution's bundled npm, so we run `node <cli.js>`
    (never a Windows `.cmd`/`.bat` shim, which `subprocess`/`execv` cannot spawn safely)."""
    rels = [
        node.parent / "node_modules" / "npm" / "bin" / f"{tool}-cli.js",            # Windows / self-contained
        node.parent.parent / "lib" / "node_modules" / "npm" / "bin" / f"{tool}-cli.js",  # POSIX prefix layout
        node.parent / "lib" / "node_modules" / "npm" / "bin" / f"{tool}-cli.js",
    ]
    for r in rels:
        if r.is_file():
            return r
    return None


def _find_tool(info: NodeInfo, tool: str) -> list[str]:
    cli = _cli_js(info.node, tool)
    if cli is not None:
        return [str(info.node), str(cli)]
    # fallback: a plain executable beside node (rare; POSIX distros keep the cli.js above)
    for cand_name in _exe_names(tool):
        beside = info.node.parent / cand_name
        if beside.is_file():
            return [str(beside)]
    raise ToolchainMissing(_build_report(
        f"the resolved Node at {info.node} has no bundled `{tool}` "
        f"(`pip install --force-reinstall nodejs-wheel-binaries`, or reinstall Node)."
    ))


def find_npm(info: NodeInfo) -> list[str]:
    """A spawnable command PREFIX for npm (`[node, npm-cli.js]`), beside the resolved Node."""
    return _find_tool(info, "npm")


def find_npx(info: NodeInfo) -> list[str]:
    """A spawnable command PREFIX for npx (`[node, npx-cli.js]`), beside the resolved Node."""
    return _find_tool(info, "npx")


# --------------------------------------------------------------------------- the vendored scaffolder


def find_scaffolder() -> Path:
    """The vendored `create-pyths-app` payload directory (`pythscribe/_web/create-pyths-app/`, present
    in every wheel and the checkout; mirror-gated by scripts/verify_scaffolder_mirror.py). `pyths new`
    runs `node <dir>/index.js`. Dev fallback: `packages/create-pyths-app/` (source checkout)."""
    if (_VENDORED_SCAFFOLDER / "index.js").is_file():
        return _VENDORED_SCAFFOLDER
    dev = _REPO_ROOT / "packages" / SCAFFOLDER_DIRNAME
    if (dev / "index.js").is_file():
        return dev
    raise ToolchainMissing(
        f"pyths: the vendored create-pyths-app scaffolder is missing from {_VENDORED_SCAFFOLDER} "
        "(damaged install? `pip install --force-reinstall pythscribe`)."
    )
