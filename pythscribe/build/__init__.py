"""`python -m pythscribe.build <module.py>` -- the explicit build step (plan §6).

For every statically-readable `@wasm` def in the module (found by parsing, NOT by
importing -- the build must not execute user code), run the pinned `pyths compile
--target js+wasm` on the extracted kernel and lay a verified artifact directory beside the
source (`artifacts.py` documents the layout). Idempotent: an artifact whose manifest
verifies against the current kernel source and compiler pin is left untouched.

Hard rules (each is a gate, not a nicety):
  * the compiler must actually emit a `.wasm` -- a JS-only result is a BUILD ERROR, never
    a silently-JS artifact (that would be the unrouted-function failure in disguise);
  * the glue's bare `pyths-runtime` IMPORT SPECIFIERS (only those) are rewritten to a
    relative runtime copy, and the copy is the transitive import closure of what the glue
    actually imports (sealed per artifact for M0 -- a shared, content-addressed runtime is
    an M1/M2 item; see review R1/N1), with LF line endings and no dangling sourcemap refs;
  * every text file is written with LF so the manifest hashes are platform-independent;
  * the reported `pyths --version` must equal the pin.
"""
from __future__ import annotations

import json
import os
import re
import shutil
import subprocess
import sys
from pathlib import Path

from .._pin import COMPILER_COMMIT, COMPILER_VERSION, MANIFEST_SCHEMA
from .._static import find_wasm_defs, kernel_source, sha256_text
from ..artifacts import (
    MANIFEST_NAME,
    ArtifactError,
    ArtifactInfo,
    artifact_dir_for,
    loadability_problem,
    manifest_self_hash,
    sha256_file,
    verify,
)
from .optimizer import Optimizer, compiler_pass_off_env, optimize_artifact_wasm, resolve_wasm_opt

__all__ = [
    "BuildError", "build_module", "build_kernel", "find_pyths", "bundled_pyths", "find_runtime_dir",
    "strip_trailing_sourcemap_comment", "SOURCE_INSTALL_NOTICE", "LAUNCHER_PROBE_ENV", "LAUNCHER_PROBE_REPLY",
    "artifact_is_fresh",
]

PYTHS_ENV = "PYTHSCRIBE_PYTHS"
RUNTIME_ENV = "PYTHSCRIBE_RUNTIME_DIR"
_REPO_ROOT = Path(__file__).resolve().parents[2]
RUNTIME_DIRNAME = "pyths-runtime"
# M0.2 (spec 13-09-26): the prepared `pyths-runtime` npm payload vendored into the wheel as package
# data -- the SAME bytes as `dist/runtime-payload/` and the published tarball (raw-byte mirror gate:
# scripts/verify_runtime_mirror.py). This is what makes `pyths build` node/npm/network-free.
_VENDORED_RUNTIME = Path(__file__).resolve().parents[1] / "_runtime" / RUNTIME_DIRNAME


class BuildError(RuntimeError):
    pass


def _write_lf(path: Path, text: str) -> None:
    path.write_text(text, encoding="utf-8", newline="\n")


# M1.2 (spec 13-09-26): the native compiler BUNDLED into the wheel as package data at
# pythscribe/_bin/pyths[.exe] (staged from the finalized release binary B_t by the cibuildwheel
# leg; gitignored in the checkout). Resolution order below. The `pyths` console script
# (`pythscribe._launcher`) is a Python LAUNCHER that forwards to this binary -- so a PATH search
# must never hand the launcher back to itself (T1: the recursion class), which is why every PATH
# candidate goes through launcher-exclusion + native validation before it is accepted.
_BUNDLED_DIR = Path(__file__).resolve().parents[1] / "_bin"
_BINARY_NAMES = ("pyths.exe", "pyths") if os.name == "nt" else ("pyths", "pyths.exe")
# The probe protocol: `find_pyths()` runs a PATH candidate's `--version` with this variable set.
# The native compiler ignores it and prints `pyths <ver>`; ANY pythscribe launcher (this venv's
# or another's) answers `LAUNCHER_PROBE_REPLY` and exits 3 WITHOUT forwarding -- so a launcher
# can never pass the probe and the probe itself can never recurse.
LAUNCHER_PROBE_ENV = "PYTHSCRIBE_LAUNCHER_PROBE"
LAUNCHER_PROBE_REPLY = "pythscribe-launcher"
_VERSION_LINE = re.compile(r"^pyths (\d+\.\d+\.\d+\S*)\s*$")
# set by pythscribe._launcher.main() to the running launcher's own argv[0] (an extra exclusion)
_RUNNING_LAUNCHER_ARGV0: Path | None = None

SOURCE_INSTALL_NOTICE = (
    "no prebuilt compiler for this platform / source install: `pyths build` and compile-on-first-call "
    "are unavailable; prebuilt `__pythscribe__/` artifacts and the Python fallback still work "
    f"(set {PYTHS_ENV}=<path to a `cargo build --release --bin pyths` binary> to use one)"
)


def bundled_pyths() -> Path | None:
    """The in-wheel binary (absolute, normalized) or None (source install / no wheel for this platform)."""
    for name in _BINARY_NAMES:
        p = _BUNDLED_DIR / name
        if p.is_file():
            return Path(os.path.normpath(p.resolve()))
    return None


def _launcher_paths() -> list[Path]:
    """Every path the pythscribe `pyths` console script can have in THIS interpreter's environment
    (venv + base prefix scripts dirs, `pyths`, `pyths.exe`, `pyths-script.py`), plus the running
    launcher's own argv[0]. A PATH candidate that is one of these is the launcher by identity --
    rejected before anything is executed."""
    import sysconfig

    dirs: list[Path] = []
    for d in (sysconfig.get_path("scripts"), os.path.join(sys.prefix, "Scripts"), os.path.join(sys.prefix, "bin"),
              os.path.join(sys.base_prefix, "Scripts"), os.path.join(sys.base_prefix, "bin")):
        if d:
            dirs.append(Path(d))
    out: list[Path] = []
    for d in dirs:
        for name in ("pyths", "pyths.exe", "pyths-script.py", "pyths.cmd", "pyths.bat"):
            out.append(d / name)
    if _RUNNING_LAUNCHER_ARGV0 is not None:
        out.append(_RUNNING_LAUNCHER_ARGV0)
    return out


def _same(a: Path, b: Path) -> bool:
    try:
        return a.exists() and b.exists() and os.path.samefile(a, b)
    except OSError:
        return False


def _reject_launcher(cand: Path) -> str | None:
    """Why `cand` must NOT be accepted as the compiler (None == may be probed). Never executes it."""
    low = cand.name.lower()
    if low.endswith((".py", ".pyw", "-script.py", ".cmd", ".bat")):
        return "a Python/script wrapper, not the compiler"
    for lp in _launcher_paths():
        if _same(cand, lp):
            return "the pythscribe `pyths` console script (launcher) itself"
    try:
        with open(cand, "rb") as f:
            head = f.read(4)
    except OSError as e:
        return f"unreadable ({e})"
    if head[:2] == b"#!":
        return "a text shebang wrapper (the console script), not a native executable"
    from ._native import sniff_format

    if sniff_format(head) is None:
        return "not a native executable (no ELF/Mach-O/PE magic)"
    return None


def _probe_native(cand: Path) -> str | None:
    """Run `cand --version` under the probe protocol. None == accepted (prints `pyths <ver>`)."""
    env = {**os.environ, LAUNCHER_PROBE_ENV: "1"}
    try:
        out = subprocess.run([str(cand), "--version"], capture_output=True, text=True, check=False, env=env, timeout=20)
    except (OSError, subprocess.SubprocessError) as e:
        return f"`--version` did not run ({e})"
    first = (out.stdout or "").strip().splitlines()[:1]
    if out.returncode != 0 or not first:
        if LAUNCHER_PROBE_REPLY in (out.stdout + out.stderr):
            return "a pythscribe launcher (answered the probe), not the compiler"
        return f"`--version` failed (exit {out.returncode}): {(out.stderr or out.stdout).strip()[:200]}"
    if not _VERSION_LINE.match(first[0]):
        return f"`--version` printed {first[0]!r}, not `pyths <version>`"
    return None


def _path_candidates(name: str = "pyths") -> list[Path]:
    """Every `pyths` executable on PATH, in PATH order -- absolute PATH entries only, never the cwd
    (the same discipline as the compiler's own `procutil::resolve_program`). `shutil.which` returns
    only the FIRST hit, which in a venv is the console script; we need to look past it."""
    exts = [""]
    if os.name == "nt":
        pathext = [e.lower() for e in os.environ.get("PATHEXT", ".EXE;.BAT;.CMD").split(os.pathsep) if e]
        exts = pathext + [""]
    seen: set[str] = set()
    out: list[Path] = []
    for d in os.environ.get("PATH", "").split(os.pathsep):
        if not d or not os.path.isabs(d):
            continue
        for ext in exts:
            p = Path(d) / f"{name}{ext}"
            key = os.path.normcase(os.path.normpath(str(p)))
            if key in seen:
                continue
            seen.add(key)
            if p.is_file():
                out.append(p)
    return out


def find_pyths() -> Path:
    """Resolve the compiler (plan M1.2): `PYTHSCRIBE_PYTHS` -> the BUNDLED `pythscribe/_bin/pyths[.exe]`
    -> `<repo>/target/release` (dev checkout) -> PATH, with launcher-exclusion + native validation.
    Never returns the `pyths` console script; never recurses (the probe protocol above)."""
    from ._native import NativeFormatError, host_mismatch, inspect_binary

    cand = os.environ.get(PYTHS_ENV)
    if cand:
        p = Path(cand)
        if not p.is_file():
            raise BuildError(f"{PYTHS_ENV}={cand!r} is not a file")
        for lp in _launcher_paths():
            if _same(p, lp):
                raise BuildError(f"{PYTHS_ENV}={cand!r} is the pythscribe `pyths` launcher, not the compiler (point it at a native `pyths` binary)")
        return Path(os.path.normpath(p.resolve()))
    bundled = bundled_pyths()
    if bundled is not None:
        try:
            with open(bundled, "rb") as f:
                info = inspect_binary(f.read())
        except NativeFormatError as e:
            raise BuildError(f"bundled compiler {bundled} is not a native executable ({e}); reinstall pythscribe") from None
        mismatch = host_mismatch(info)
        if mismatch:
            raise BuildError(mismatch)
        return bundled
    for name in _BINARY_NAMES:
        p = _REPO_ROOT / "target" / "release" / name
        if p.is_file():
            return p
    rejected: list[str] = []
    for c in _path_candidates():
        why = _reject_launcher(c) or _probe_native(c)
        if why is None:
            return Path(os.path.normpath(c.resolve()))
        rejected.append(f"{c}: {why}")
    detail = ("; PATH candidates rejected: " + " | ".join(rejected)) if rejected else ""
    if not _BUNDLED_DIR.is_dir():
        raise BuildError(f"pyths compiler not found -- {SOURCE_INSTALL_NOTICE}{detail}")
    raise BuildError(
        f"pyths compiler not found: the bundled binary is missing from {_BUNDLED_DIR} (damaged install? "
        f"`pip install --force-reinstall pythscribe`), no dev build at {_REPO_ROOT / 'target' / 'release'}, "
        f"and no native `pyths` on PATH; or set {PYTHS_ENV}=<path to pyths>{detail}"
    )


def find_runtime_dir(pyths: Path | None = None) -> Path:
    """The `pyths-runtime` `src/` directory the artifact's runtime closure is copied from.
    Order (plan M0.2): `PYTHSCRIBE_RUNTIME_DIR` -> the VENDORED in-package payload
    (`pythscribe/_runtime/pyths-runtime/src`, present in every wheel and in the checkout) ->
    the external dev fallback (`<repo>/runtime/src`, only when the vendored copy is absent)."""
    cand = os.environ.get(RUNTIME_ENV)
    if cand:
        p = Path(cand)
        if (p / "index.js").is_file():
            return p
        raise BuildError(f"{RUNTIME_ENV}={cand!r} has no index.js")
    vendored = _VENDORED_RUNTIME / "src"
    if (vendored / "index.js").is_file():
        return vendored
    roots = [_REPO_ROOT]
    if pyths is not None:
        parents = Path(pyths).resolve().parents
        if len(parents) > 2:
            roots.append(parents[2])  # <repo>/target/release/pyths -> <repo>
    for root in roots:
        p = root / "runtime" / "src"
        if (p / "index.js").is_file():
            return p
    raise BuildError(
        f"pyths-runtime sources not found (vendored copy {vendored} is absent); set {RUNTIME_ENV}=<path to runtime/src>"
    )


def pyths_version(pyths: Path) -> str:
    out = subprocess.run([str(pyths), "--version"], capture_output=True, text=True, check=False)
    if out.returncode != 0:
        raise BuildError(f"`{pyths} --version` failed: {out.stderr.strip()}")
    # "pyths 0.2.4"
    return out.stdout.strip().split()[-1]


# Only IMPORT/EXPORT SPECIFIERS are rewritten -- `from "..."`, `import "..."`, `import("...")`
# -- never an arbitrary string literal in user code (review R1/SF13).
_RUNTIME_SPEC = re.compile(
    r"""(?P<pre>\b(?:from|import)\s*\(?\s*)(?P<q>["'])pyths-runtime(?P<sub>/[A-Za-z0-9_./-]+)?(?P=q)"""
)


def _rewrite_runtime_imports(js: str) -> str:
    def sub(m: re.Match[str]) -> str:
        q, sub_path = m.group("q"), m.group("sub")
        if not sub_path:
            target = f"./{RUNTIME_DIRNAME}/index.js"
        elif sub_path.endswith(".js") or sub_path.endswith(".mjs"):
            target = f"./{RUNTIME_DIRNAME}{sub_path}"
        else:
            target = f"./{RUNTIME_DIRNAME}{sub_path}.js"
        return f"{m.group('pre')}{q}{target}{q}"

    return _RUNTIME_SPEC.sub(sub, js)


_REL_SPEC = re.compile(r"""(?:\bfrom\s*|\bimport\s*\(?\s*)(["'])(\.{1,2}/[^"']+)\1""")
_SOURCEMAP_COMMENT_PREFIX = "//# sourceMappingURL="


def strip_trailing_sourcemap_comment(text: str, basename: str) -> str:
    """M0.4 (spec 13-09-26, blocker #5): the ARTIFACT-ONLY sourcemap strip, made sound.

    Strips the map comment IFF `text` ends LITERALLY with ``\\n//# sourceMappingURL=<basename>.map``
    followed by at most ONE terminal ``\\n`` (or is exactly that directive, for a comment-only
    file) -- for ``core.js``, ``//# sourceMappingURL=core.js.map`` -- which is the position + name
    the `runtime_maps.rs` invariant guarantees for every shipped runtime `.js`. NOTHING else is
    touched: a directive with trailing spaces, an indented one, one followed by a blank line, a
    `sourceMappingURL` line anywhere else (inside a template literal, mid-file) -- all preserved
    byte-for-byte (codex M0 SF2: no `rstrip`/`strip` slack around the match). (The old MULTILINE
    regex `^\\s*//[#@]\\s*sourceMappingURL=.*$` had no lexical context and deleted such lines inside
    string literals -- the codex reproducer, validation §C3.) Applies only to the per-artifact copy
    under `__pythscribe__/`, never to the vendored payload."""
    directive = f"{_SOURCEMAP_COMMENT_PREFIX}{basename}.map"
    if text in (directive, directive + "\n"):
        return ""
    for suffix in (f"\n{directive}\n", f"\n{directive}"):
        if text.endswith(suffix):
            return text[: len(text) - len(suffix) + 1]  # keep the newline that preceded the directive
    return text


def _relative_specifiers(js: str) -> list[str]:
    return [m.group(2) for m in _REL_SPEC.finditer(js)]


def _runtime_closure(runtime_src: Path, entry_specs: list[str]) -> list[Path]:
    """Transitive closure of relative imports inside the runtime, starting from the
    `./pyths-runtime/<x>` specifiers the glue/entry use. Paths are relative to runtime_src."""
    todo: list[Path] = []
    for spec in entry_specs:
        prefix = f"./{RUNTIME_DIRNAME}/"
        if spec.startswith(prefix):
            todo.append(Path(spec[len(prefix):]))
    seen: set[Path] = set()
    while todo:
        rel = todo.pop()
        rel = Path(os.path.normpath(rel.as_posix()))
        if rel in seen:
            continue
        p = runtime_src / rel
        if not p.is_file():
            raise BuildError(f"runtime import target missing: {rel.as_posix()} (under {runtime_src})")
        seen.add(rel)
        js = p.read_text(encoding="utf-8")
        for spec in _relative_specifiers(js):
            todo.append(Path(os.path.normpath((rel.parent / spec).as_posix())))
    if not seen and any(s.startswith(f"./{RUNTIME_DIRNAME}") for s in entry_specs):
        raise BuildError("the emitted glue imports from pyths-runtime but nothing resolved; refusing an empty runtime copy")
    return sorted(seen)


_ANY_SPEC = re.compile(r"""(?:\bfrom\s*|\bimport\s*\(?\s*)(["'])([^"']+)\1""")
_DYNAMIC_NONLITERAL = re.compile(r"""\bimport\s*\(\s*(?!["'])[^)\s]""")
_NEW_URL = re.compile(r"""new\s+URL\(\s*(["'])([^"']+)\1\s*,\s*import\.meta\.url""")


def _check_artifact_specifiers(adir: Path, js_files: list[Path]) -> None:
    """ONE post-condition over every shipped JS file (R2/S6): every import/export specifier
    and every `new URL(x, import.meta.url)` must be RELATIVE and resolve to a file inside the
    artifact directory; no bare `pyths-runtime` (or any other bare/npm specifier) survives;
    no non-literal dynamic import (the closure cannot follow it). This makes "every byte the
    browser loads is listed" true by construction, not by two regexes agreeing."""
    for js in js_files:
        text = js.read_text(encoding="utf-8")
        rel = js.relative_to(adir).as_posix()
        if _DYNAMIC_NONLITERAL.search(text):
            raise BuildError(f"{rel}: non-literal dynamic import() cannot be sealed into an artifact")
        specs = [m.group(2) for m in _ANY_SPEC.finditer(text)] + [m.group(2) for m in _NEW_URL.finditer(text)]
        for spec in specs:
            if spec.startswith("node:"):
                continue  # the glue's Node-only branch (`await import('node:fs/promises')`) is never taken in a browser
            if not (spec.startswith("./") or spec.startswith("../")):
                raise BuildError(f"{rel}: bare specifier {spec!r} would not resolve in a browser")
            target = (js.parent / spec).resolve()
            try:
                target.relative_to(adir.resolve())
            except ValueError:
                raise BuildError(f"{rel}: {spec!r} escapes the artifact directory") from None
            if not target.is_file():
                raise BuildError(f"{rel}: {spec!r} -> {target.name} is not in the artifact")


def _write_artifact_root_gitattributes(adir: Path) -> None:
    """`__pythscribe__/.gitattributes` (a sibling of the per-function dirs, OUTSIDE the hashed
    tree) so a user who commits artifacts on Windows does not get CRLF-converted checkouts
    that fail every hash (R2/S9 -- the repo's own .gitattributes fix, shipped to users)."""
    ga = adir.parent / ".gitattributes"
    want = "# pythscribe artifacts are hashed byte-for-byte: never convert line endings\n* -text\n"
    if not ga.is_file() or ga.read_text(encoding="utf-8") != want:
        _write_lf(ga, want)


def _copy_runtime(src: Path, dst: Path, entry_specs: list[str]) -> list[Path]:
    """The runtime closure the glue/entry actually import. EMPTY is legitimate (M1.5): a pure
    scalar kernel's glue needs no runtime helper at all, so no `pyths-runtime/` directory is
    written and the manifest records `runtime_dir: null`. Refused only when the glue DID
    import from pyths-runtime and nothing resolved (a rewrite/closure bug)."""
    if dst.exists():
        shutil.rmtree(dst)
    copied: list[Path] = []
    for rel in _runtime_closure(src, entry_specs):
        text = (src / rel).read_text(encoding="utf-8")
        # no dangling `//# sourceMappingURL` (R1/N3) -- the sound, last-line-only strip (M0.4)
        text = strip_trailing_sourcemap_comment(text, rel.name).rstrip("\n") + "\n"
        out = dst / rel
        out.parent.mkdir(parents=True, exist_ok=True)
        _write_lf(out, text)
        copied.append(out)
    return copied


def artifact_is_fresh(manifest: dict, optimizer: Optimizer) -> bool:
    """FRESHNESS (never integrity -- that is `verify()`): built by the pinned compiler AND under the
    SAME ambient optimizer identity (M3). A manifest without an `optimizer` field (pre-M3) or with a
    different id (none->X, X->none, X->Y) is stale -> rebuild. Shared by the explicit build and the
    JIT cache's `_valid_at` so the two freshness checks cannot drift."""
    if manifest.get("compiler", {}).get("version") != COMPILER_VERSION:
        return False
    opt = manifest.get("optimizer")
    return isinstance(opt, dict) and opt.get("id") == optimizer.id


def build_kernel(
    source_file: Path,
    name: str,
    ksrc: str,
    *,
    pyths: Path | None = None,
    force: bool = False,
    quiet: bool = False,
    optimizer: Optimizer | None = None,
) -> ArtifactInfo:
    source_file = Path(source_file).resolve()
    adir = artifact_dir_for(source_file, name)
    src_sha = sha256_text(ksrc)
    optimizer = optimizer or resolve_wasm_opt()  # the ambient identity: freshness input + the pass's binary

    if not force and (adir / MANIFEST_NAME).is_file():
        try:
            info = verify(adir, function=name, expected_source_sha256=src_sha)
            # up to date == M3 freshness (pin + optimizer id + source sha) AND M2.1 LOADABLE by this
            # runtime (a pre-ABI artifact of the same pin -- no `pyths.abi` section -- is rebuilt, not kept)
            if artifact_is_fresh(info.manifest, optimizer) and loadability_problem(info) is None:
                if not quiet:
                    print(f"pythscribe build: `{name}` up to date -> {adir}")
                return info
        except (ArtifactError, OSError):
            pass  # rebuild below

    pyths = pyths or find_pyths()
    ver = pyths_version(pyths)
    if ver != COMPILER_VERSION:
        raise BuildError(f"pyths reports version {ver!r} but pythscribe is pinned to {COMPILER_VERSION!r}")
    runtime_src = find_runtime_dir(pyths)

    if adir.exists():
        shutil.rmtree(adir)
    adir.mkdir(parents=True)
    ps = adir / f"{name}.ps"
    _write_lf(ps, ksrc)
    entry = adir / f"{name}.js"
    glue = adir / f"{name}.glue.js"
    wasm = adir / f"{name}.wasm"

    cmd = [str(pyths), "compile", str(ps), "--target", "js+wasm", "-o", str(entry), "--no-dts", "--quiet"]
    # M3: the compiler's OWN wasm-opt pass is off by contract (PYTHS_WASM_OPT = an absolute path that
    # does not exist -> `resolve_program` refuses it, never falls back to PATH); the pass is Python-owned below.
    proc = subprocess.run(cmd, capture_output=True, text=True, check=False, cwd=str(adir), env=compiler_pass_off_env(adir))
    if proc.returncode != 0:
        shutil.rmtree(adir, ignore_errors=True)
        raise BuildError(f"pyths compile failed for `{name}`:\n{proc.stderr.strip() or proc.stdout.strip()}")
    for p in (entry, glue, wasm):
        if not p.is_file():
            shutil.rmtree(adir, ignore_errors=True)
            raise BuildError(
                f"`{name}`: compiler did not emit {p.name}; the kernel was NOT compiled to WASM "
                "(a JS-only result is refused -- make the function WASM-eligible: scalar return, "
                "annotated params)"
            )
    glue_js = glue.read_text(encoding="utf-8")
    if "WebAssembly.instantiate" not in glue_js:
        shutil.rmtree(adir, ignore_errors=True)
        raise BuildError(f"`{name}`: glue has no WebAssembly loader; refusing a non-WASM artifact")
    # M2.1 (spec 13-09-26 §5.6): the module the pinned compiler just emitted must carry a `pyths.abi`
    # section THIS runtime accepts (major + both layout strings). The version pin above binds the
    # release identity; this binds the LAYOUT contract -- a compiler/runtime skew within the same
    # version is a build error, never an artifact the server path later refuses.
    from ..runtime import abi as _abi

    abi_problem = _abi.module_problem(wasm.read_bytes(), name=name)
    if abi_problem is not None:
        shutil.rmtree(adir, ignore_errors=True)
        raise BuildError(f"`{name}`: the compiler's WASM ABI does not match this pythscribe runtime: {abi_problem}")
    glue_js = _rewrite_runtime_imports(glue_js)
    entry_js = _rewrite_runtime_imports(entry.read_text(encoding="utf-8"))
    _write_lf(glue, glue_js)
    _write_lf(entry, entry_js)
    try:
        copied = _copy_runtime(runtime_src, adir / RUNTIME_DIRNAME, _relative_specifiers(glue_js) + _relative_specifiers(entry_js))
        _check_artifact_specifiers(adir, [entry, glue, *copied])
    except BuildError:
        shutil.rmtree(adir, ignore_errors=True)  # never leave a half-built directory behind (M1.5: it used to, on the closure path)
        raise
    _write_artifact_root_gitattributes(adir)

    # M3 ordering (rev-4 S2): the Python-owned optimizer pass runs AFTER compile + the glue/entry
    # rewrites and BEFORE the file-hashing block, so the (possibly optimized) .wasm bytes, the
    # `optimizer` field and the manifest self-hash are written by the same final manifest step --
    # never a post-manifest mutation. Degrades (applied: false + error), never raises.
    opt_record = optimize_artifact_wasm(wasm, adir, optimizer)

    files = {
        p.relative_to(adir).as_posix(): sha256_file(p)
        for p in sorted(adir.rglob("*"))
        if p.is_file() and p.name != MANIFEST_NAME
    }
    manifest = {
        "schema": MANIFEST_SCHEMA,
        "function": name,
        "source_file": source_file.name,
        "source_sha256": src_sha,
        "target": "js+wasm",
        "entry": entry.name,
        "glue": glue.name,
        "wasm": wasm.name,
        "runtime_dir": RUNTIME_DIRNAME if copied else None,
        "compiler": {"name": "pyths", "version": ver, "commit": COMPILER_COMMIT},
        "optimizer": opt_record.as_manifest(),
        "files": files,
    }
    manifest["manifest_sha256"] = manifest_self_hash(manifest)
    _write_lf(adir / MANIFEST_NAME, json.dumps(manifest, indent=2, sort_keys=True) + "\n")
    info = verify(adir, function=name, expected_source_sha256=src_sha)  # self-check the thing we wrote
    if not quiet:
        opt_note = f"optimized by {opt_record.id}" if opt_record.applied else f"unoptimized: {opt_record.error or 'no wasm-opt found (install binaryen on PATH or set PYTHS_WASM_OPT)'}"
        print(f"pythscribe build: `{name}` -> {adir} ({wasm.stat().st_size} B wasm, {len(files)} files; {opt_note})")
    return info


def build_module(path: str | os.PathLike, *, force: bool = False, quiet: bool = False) -> list[ArtifactInfo]:
    source_file = Path(path).resolve()
    if not source_file.is_file():
        raise BuildError(f"no such module file: {source_file}")
    module_source = source_file.read_text(encoding="utf-8")
    defs = find_wasm_defs(module_source, str(source_file))  # raises on duplicate/shadowed names
    if not defs:
        raise BuildError(f"{source_file}: no top-level @wasm functions found")
    optimizer = resolve_wasm_opt()  # ONE ambient identity per build invocation (one probe, not one per kernel)
    out: list[ArtifactInfo] = []
    for node in defs:
        out.append(build_kernel(source_file, node.name, kernel_source(module_source, node), force=force, quiet=quiet, optimizer=optimizer))
    return out


def main(argv: list[str] | None = None, prog: str = "python -m pythscribe.build") -> int:
    import argparse

    ap = argparse.ArgumentParser(prog=prog, description=__doc__.split("\n\n")[0])
    ap.add_argument("modules", nargs="+", help="Python source files containing @wasm functions")
    ap.add_argument("--force", action="store_true", help="rebuild even if the artifact is up to date")
    ap.add_argument("--quiet", action="store_true")
    ns = ap.parse_args(argv)
    try:
        for m in ns.modules:
            build_module(m, force=ns.force, quiet=ns.quiet)
    except BuildError as e:
        print(f"pythscribe build: ERROR: {e}", file=sys.stderr)
        return 2
    return 0
