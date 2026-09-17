"""The `pyths` console script of the pip wheel (spec 13-09-26, plan M1.1).

`[project.scripts] pyths = "pythscribe._launcher:main"`. It is a thin LAUNCHER, not a second CLI:

  * Python-implemented subcommands: `build` (-> `pythscribe.build.main`) and the M7 set
    `new / install / dev / node / npm` (`_web_command`: every one resolves Node through
    `pythscribe._web.find_node()` first; without a usable Node the §3.2 report is printed at this
    boundary and the exit is 1, never a traceback) plus `doctor` (the M7.2b environment report).
    None of these collides with the native enum (`compile / expand / check / run / init / test /
    fmt / lint / bundle / cache`) -- native `init` is forwarded unchanged; the parity scaffold is `new`.
  * EVERYTHING ELSE is forwarded VERBATIM to the bundled native binary resolved by
    `pythscribe.build.find_pyths()` (M1.2): every native subcommand and flag (`compile --emit-cert`
    is a compile FLAG), `--version`, the global `--quiet` / `--verbose`. argv and streams are
    preserved and the exit code propagated: `os.execv` on POSIX (the launcher process IS replaced),
    `subprocess` + exit-code propagation on Windows (no execv semantics there).
  * `pyths --help` / `pyths` (no args): the native help output FOLLOWED BY a launcher section
    listing `build` + the M7 commands, so neither surface hides the other.
  * `pyths run` / `pyths test` need Node (T6: they execute the compiled program with `node`); with
    no `node` on PATH the launcher prints the graceful message instead of the binary's spawn error.
  * It NEVER re-enters itself: the resolver excludes the console script by path, by shebang/magic
    and by the probe protocol (`PYTHSCRIBE_LAUNCHER_PROBE`), and the launcher refuses to forward to
    a path that is itself. When invoked under the probe it answers `pythscribe-launcher` and exits 3
    without forwarding -- that reply is what makes a launcher fail `find_pyths()`'s native probe.
"""
from __future__ import annotations

import os
import shutil
import subprocess
import sys
from pathlib import Path

PYTHON_COMMANDS = ("build", "new", "install", "dev", "node", "npm", "doctor")
M7_COMMANDS = ("new", "install", "dev", "node", "npm", "doctor")
NATIVE_COMMANDS = ("compile", "expand", "check", "run", "init", "test", "fmt", "lint", "bundle", "cache")
NODE_NEEDING = ("run", "test")
GLOBAL_FLAGS = ("--quiet", "--verbose")
EXIT_LAUNCHER = 3

LAUNCHER_HELP = """
pythscribe launcher commands (Python; not part of the native `pyths` binary above):
  build <module.py>...   compile every @wasm def to a verified js+wasm artifact (`pyths build --help`)
  new <app>              scaffold a .ps React/Next frontend (vendored create-pyths-app; needs Node)
  install                `npm install` in the app dir with the resolved npm (hits the npm registry)
  dev                    run the app's dev server (`npm run dev`); wires the pip compiler via PYTHS_BIN
  node / npm ...         run the resolved Node / npm verbatim (`pyths npm run build`)
  doctor                 what this machine can do (compiler, Node, optimiser, wasmtime, adapters); --json / --strict
Everything else is forwarded verbatim to the bundled native compiler.
"""

NODE_MESSAGE = (
    "pyths {cmd}: this command executes the compiled program with Node.js, and no `node` is on PATH.\n"
    "  `pyths build` / `pyths compile` (JS + WASM), `@wasm` kernels (wasmtime, in-process) and the Gradio /\n"
    "  Streamlit adapters do NOT need Node. To use `pyths {cmd}` install Node (https://nodejs.org) or put it on PATH."
)


def _resolve_binary() -> Path:
    from .build import BuildError, find_pyths
    import pythscribe.build as build_mod

    build_mod._RUNNING_LAUNCHER_ARGV0 = Path(sys.argv[0]) if sys.argv and sys.argv[0] else None
    try:
        binary = find_pyths()
    except BuildError as e:
        raise SystemExit(f"pyths: {e}") from None
    me = Path(sys.argv[0]) if sys.argv and sys.argv[0] else None
    if me is not None:
        try:
            if me.exists() and os.path.samefile(binary, me):
                raise SystemExit(f"pyths: refusing to re-enter the launcher ({binary} is this console script, not the compiler)")
        except OSError:
            pass
    return binary


def forward(binary: Path, args: list[str]) -> int:
    """Hand the whole invocation to the native binary. POSIX: execv (never returns). Windows:
    child process with inherited streams, exit code propagated."""
    cmd = [str(binary), *args]
    if os.name != "nt":
        try:
            os.execv(str(binary), cmd)  # noqa: S606 -- the launcher's whole purpose
        except OSError as e:  # a corrupt / non-executable bundled binary: clean error, not a traceback
            print(
                f"pyths: the bundled compiler at {binary} could not be executed ({e.strerror or e}); "
                f"the install may be damaged -- reinstall pythscribe.",
                file=sys.stderr,
            )
            return 1
        return 0  # pragma: no cover -- execv replaces the process on success
    try:
        return subprocess.run(cmd, check=False).returncode
    except KeyboardInterrupt:
        return 130
    except OSError as e:  # same clean-error contract on Windows
        print(
            f"pyths: the bundled compiler at {binary} could not be executed ({e.strerror or e}); "
            f"the install may be damaged -- reinstall pythscribe.",
            file=sys.stderr,
        )
        return 1


def _split(argv: list[str]) -> tuple[list[str], str | None, list[str]]:
    """(leading global flags, subcommand or None, everything after the subcommand)."""
    i = 0
    globs: list[str] = []
    while i < len(argv) and argv[i] in GLOBAL_FLAGS:
        globs.append(argv[i])
        i += 1
    if i >= len(argv):
        return globs, None, []
    return globs, argv[i], argv[i + 1:]


def _help(binary: Path, argv: list[str]) -> int:
    """Native help/usage first (captured, re-emitted on the same streams), then the launcher section."""
    p = subprocess.run([str(binary), *argv], capture_output=True, text=True, check=False)
    sys.stdout.write(p.stdout)
    sys.stderr.write(p.stderr)
    (sys.stdout if p.returncode == 0 else sys.stderr).write(LAUNCHER_HELP)
    sys.stdout.flush()
    sys.stderr.flush()
    return p.returncode


def _importable(modname: str) -> bool:
    """True iff `modname` imports. The seam doctor's wasmtime/gradio/streamlit probes go through, so a
    test can simulate absence (monkeypatch) and a mutant hardcoding it goes RED in that line's test."""
    import importlib.util

    try:
        return importlib.util.find_spec(modname) is not None
    except (ImportError, ValueError):
        return False


def _probe_compiler() -> dict:
    from . import build as _pyb
    from ._pin import ACCEPTED_COMPILER_RANGE, COMPILER_VERSION
    from .build import BuildError
    from .runtime.abi import SUPPORTED_ABI_MAJOR

    # The doctor names the compat window a runtime will LOAD -- the accepted compiler range (Layer 2)
    # and the ABI major (Layer 1) -- not just the exact pin, so a mismatch is diagnosable at a glance.
    compat = f"accepts {ACCEPTED_COMPILER_RANGE}, ABI major {SUPPORTED_ABI_MAJOR}"
    try:
        p = _pyb.find_pyths()
        ver = _pyb.pyths_version(p)
        ok = ver == COMPILER_VERSION
        return {
            "key": "compiler", "label": "Compiler", "present": ok,
            "detail": (f"pyths {ver} at {p} (pinned {COMPILER_VERSION}; {compat})" if ok
                       else f"VERSION MISMATCH -- pyths {ver} at {p} (pinned {COMPILER_VERSION}; {compat})"),
            "unlocks": "`pyths build` / `pyths compile` to JS + WASM",
            "path": str(p), "version": ver,
        }
    except BuildError as e:
        return {
            "key": "compiler", "label": "Compiler", "present": False, "state": "not bundled",
            "detail": "source install / no wheel for this platform",
            "fix": f"reinstall a platform wheel: `pip install --force-reinstall pythscribe` ({e})",
        }


def _probe_node() -> dict:
    from . import _web

    try:
        info = _web.find_node(print_notice=False)
        return {
            "key": "node", "label": "Node", "present": True,
            "detail": f"{info.source} Node v{info.version} at {info.node}",
            "unlocks": "dev server / HMR / npm imports (`pyths new` / `install` / `dev`)",
            "path": str(info.node), "version": info.version, "source": info.source,
        }
    except _web.ToolchainMissing:
        return {
            "key": "node", "label": "Node", "present": False,
            "detail": "no usable Node resolved (PYTHS_NODE -> PATH -> vendored)",
            "fix": "install Node (https://nodejs.org) or `pip install pythscribe[web-bundled]`",
        }


def _probe_optimizer() -> dict:
    from .build import optimizer as _opt

    o = _opt.resolve_wasm_opt()
    if o.id != _opt.OPTIMIZER_NONE:
        return {
            "key": "wasm-opt", "label": "WASM optimiser", "present": True,
            "detail": f"{o.id}" + (f" at {o.path}" if o.path else ""),
            "unlocks": "size-optimised `.wasm` artifacts",
        }
    return {
        "key": "wasm-opt", "label": "WASM optimiser", "present": False,
        "detail": "artifacts are unoptimised" + (f" ({o.error})" if o.error else ""),
        # M7-8b: NO pip extra named here -- no `[optimize]` ships in this release (validation §M7-8b).
        "fix": "install binaryen's `wasm-opt` on PATH or set `PYTHS_WASM_OPT`",
    }


def _probe_wasmtime() -> dict:
    ok = _importable("wasmtime")
    return {
        "key": "wasmtime", "label": "wasmtime", "present": ok,
        "detail": "importable" if ok else "not installed",
        "unlocks": "in-process server execution of `@wasm` (sandboxed, GIL-free)",
        **({} if ok else {"fix": "`pip install pythscribe[server]`"}),
    }


def _probe_adapter(mod: str, label: str, extra: str) -> dict:
    ok = _importable(mod)
    return {
        "key": mod, "label": f"Adapter ({label})", "present": ok,
        "detail": "importable" if ok else "not installed",
        "unlocks": f"the {label} adapter",
        **({} if ok else {"fix": f"`pip install pythscribe[{extra}]`"}),
    }


def _probe_mcp() -> dict:
    # Informational (present is None): the MCP server ships in v0.3, never claimed here.
    return {"key": "mcp", "label": "MCP", "present": None,
            "detail": "not in this release (v0.3)", "unlocks": "agent tooling"}


def doctor_report() -> list[dict]:
    """Probe the REAL environment (no caching) through the SAME resolvers the commands use --
    `find_pyths`, `_web.find_node`, `resolve_wasm_opt` -- so doctor can never disagree with
    `pyths dev` about which toolchain exists (validation §M7-8)."""
    return [
        _probe_compiler(),
        _probe_node(),
        _probe_optimizer(),
        _probe_wasmtime(),
        _probe_adapter("gradio", "Gradio", "gradio"),
        _probe_adapter("streamlit", "Streamlit", "streamlit"),
        _probe_mcp(),
    ]


def _doctor(argv: list[str]) -> int:
    """M7.2b: the full environment report. Exit 0 (a report, not a gate) UNLESS `--strict`, which
    exits non-zero when any `--require`d line is absent (CI uses `--strict --require node`)."""
    import argparse
    import json as _json

    ap = argparse.ArgumentParser(prog="pyths doctor", add_help=True,
                                 description="what this machine can do (compiler, Node, optimiser, wasmtime, adapters)")
    ap.add_argument("--json", action="store_true", help="machine-readable report for tooling")
    ap.add_argument("--strict", action="store_true", help="exit non-zero when a --require line is absent")
    ap.add_argument("--require", default="", help="comma-separated keys that must be present under --strict (e.g. node,wasmtime)")
    ns = ap.parse_args(argv)

    lines = doctor_report()
    if ns.json:
        print(_json.dumps({"lines": lines}, indent=2, sort_keys=True))
    else:
        for ln in lines:
            state = ln.get("state") or ("present" if ln["present"] is True else ("n/a" if ln["present"] is None else "absent"))
            tail = f" -> {ln['unlocks']}" if ln.get("present") is True and ln.get("unlocks") else ""
            if ln["present"] is not True and ln.get("fix"):
                tail = f"; fix: {ln['fix']}"
            print(f"{ln['label']}: {state} -- {ln['detail']}{tail}")

    required = [k.strip() for k in ns.require.split(",") if k.strip()]
    if ns.strict and required:
        by_key = {ln["key"]: ln for ln in lines}
        absent = [k for k in required if not (by_key.get(k, {}).get("present") is True)]
        if absent:
            print(f"pyths doctor: --strict: required but absent: {', '.join(absent)}", file=sys.stderr)
            return 1
    return 0


def _web_command(sub: str, rest: list[str]) -> int:
    """The M7 frontend commands (`new / install / dev / node / npm`). ALL route through
    `pythscribe._web.find_node()` first; the ONLY place `ToolchainMissing` is caught is HERE (the
    launcher boundary) -- it is printed with no traceback and exits non-zero (validation §M7-5)."""
    from . import _web

    try:
        info = _web.find_node()  # prints "using <source> Node ..." / the fallback notice
        if sub == "node":
            return _spawn([str(info.node), *rest])
        if sub == "new":
            scaffolder = _web.find_scaffolder()
            return _spawn([str(info.node), str(scaffolder / "index.js"), *rest])
        npm = _web.find_npm(info)
        if sub == "npm":
            return _spawn([*npm, *rest])
        if sub == "install":
            # M7-6: `pyths install` hits the npm registry exactly like `npm install` -- state it, so
            # "fully offline" is understood as a BASE-wheel property, not the frontend dev loop's.
            print("pyths install: running `npm install`, which fetches JS dependencies from the npm "
                  "registry (the frontend dev loop needs network on both channels; `pip install "
                  "pythscribe` itself stays offline).", file=sys.stderr)
            return _spawn([*npm, "install", *rest])
        if sub == "dev":
            return _spawn([*npm, "run", "dev", *rest], env=_dev_child_env())
    except _web.ToolchainMissing as e:
        print(e.report, file=sys.stderr)
        return 1
    raise AssertionError(f"unhandled web command {sub!r}")  # pragma: no cover


def _frontend_binary() -> Path | None:
    """The absolute pip compiler `pyths dev` exports as PYTHS_BIN so the plugin's
    `resolvePythsCommand` (step 2) selects it deterministically. `find_pyths()` yields the bundled
    `_bin/pyths[.exe]` (or PYTHSCRIBE_PYTHS / dev build) -- always an absolute native binary."""
    from .build import BuildError, find_pyths

    try:
        return find_pyths()
    except BuildError:
        return None


def _dev_child_env(base: dict | None = None) -> dict:
    """The child env for `pyths dev` (testable SPOT for §M7-4): PYTHS_BIN set to the absolute pip
    compiler so the Vite/Next plugin compiles `.ps` with it -- no plugin code change. Left unset on a
    source install with no resolvable binary (the plugin then falls back to its own resolution)."""
    env = dict(base if base is not None else os.environ)
    binary = _frontend_binary()
    if binary is not None:
        env["PYTHS_BIN"] = str(binary)
    return env


def _spawn(cmd: list[str], env: dict | None = None) -> int:
    """Run a child process with inherited streams; propagate its exit code (uniform across POSIX and
    Windows -- these commands spawn Node, so execv's process-replacement buys nothing)."""
    try:
        return subprocess.run(cmd, check=False, env=env).returncode
    except KeyboardInterrupt:
        return 130
    except OSError as e:
        print(f"pyths: could not run {cmd[0]} ({e.strerror or e})", file=sys.stderr)
        return 1


def main(argv: list[str] | None = None) -> int:
    argv = list(sys.argv[1:] if argv is None else argv)
    if os.environ.get("PYTHSCRIBE_LAUNCHER_PROBE"):
        # find_pyths()'s native probe: identify as a launcher and STOP (never forward -> no recursion)
        print("pythscribe-launcher")
        return EXIT_LAUNCHER
    globs, sub, rest = _split(argv)
    if sub == "build":
        from .build import main as build_main

        extra = ["--quiet"] if "--quiet" in globs else []
        return build_main(rest + extra, prog="pyths build")
    if sub == "doctor":
        return _doctor(rest)
    if sub in M7_COMMANDS:  # new / install / dev / node / npm -- the frontend toolchain (M7.3)
        return _web_command(sub, rest)
    binary = _resolve_binary()
    if sub is None or sub in ("-h", "--help", "help"):
        return _help(binary, argv)
    if sub in NODE_NEEDING and shutil.which("node") is None:
        print(NODE_MESSAGE.format(cmd=sub), file=sys.stderr)
        return 2
    return forward(binary, argv)


if __name__ == "__main__":  # `python -m pythscribe._launcher ...` (tests; no console script needed)
    sys.exit(main())
