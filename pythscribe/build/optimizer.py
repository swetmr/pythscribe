"""The WASM optimizer (M3, spec 13-09-26-lib-selfcontained-pip-wheel): identity, resolution and the
Python-OWNED `wasm-opt -Os` pass over a freshly compiled artifact.

Three separations, each a gate (plan §M3, blocker #9):

  * INTEGRITY vs FRESHNESS. `artifacts.verify()` is integrity-only and NEVER consults the ambient
    optimizer, so a prebuilt optimized artifact is adopted on a machine with no `wasm-opt`.
    Freshness (does THIS machine's optimizer state match what the artifact was built with?) lives
    only in the explicit build (`build_kernel`) and the JIT cache KEY (`decorators._compile_to_cache`),
    both keyed on `Optimizer.id` == the manifest's `optimizer.id`.
  * ONE resolver contract. `resolve_wasm_opt()` mirrors the compiler's
    `procutil::resolve_program("wasm-opt", "PYTHS_WASM_OPT")` EXACTLY: an env override that is a
    bare name is PATH-searched (absolute PATH dirs only, never the cwd, Windows PATHEXT); a
    non-bare value must be a TRULY absolute existing file, else it is REFUSED -> no optimizer, no
    fallback to PATH; no override -> PATH search. `tests/pythscribe/test_wheel_m3_optimizer.py`
    drives the SAME fixture table as `procutil.rs`'s unit tests through this resolver.
  * The pass is Python-owned and NEVER in place (mirrors `optimize.rs::run_wasm_opt_at`). The
    compiler's own auto-pass is switched off by contract -- `build_kernel` runs `pyths compile` with
    `PYTHS_WASM_OPT=<artifact dir>/.no-wasm-opt`, an absolute path that does not exist, which
    `resolve_program` refuses (documented, SPOT-pinned by validation §F9). Then, if an optimizer is
    resolved here, `wasm-opt -Os <wasm> -o <private tmp dir>/out.wasm` runs beside the artifact; the
    candidate is assembled in that tmp dir (exit 0; size > 0; every preserved custom section --
    the compiler's `pythscribe.generated` ownership marker and, when present, `pyths.abi` -- is
    present and byte-equal to the input's, re-appended INTO THE TMP CANDIDATE if the optimizer
    stripped it) and PUBLISHED by ONE atomic `os.replace` only after the publish gate:

      - `wasmtime` importable -> `wasmtime.Module.validate(engine, FINAL bytes)` (a full decode +
        type-check of every function body) on the bytes AFTER the re-append;
      - `wasmtime` NOT importable -> the optimized output is NEVER published (a structural section
        walk is a diagnostic, not a validator: section decoding != instruction/type validation).

    Any failure -> the tmp dir is removed, the ORIGINAL unoptimized `.wasm` stays the artifact,
    `optimizer: {applied: false, error: <reason>}`, and the build succeeds (degrade, never raise).
    Only published, wasmtime-validated bytes are ever hashed into the manifest.

KNOWN LIMITATION (rev-7 SF-D, documented, not silently shipped): the only zero-crate-churn WASM
validator available to the Python build today is `wasmtime` (a `[server]` extra), so a base /
`[gradio]` / `[streamlit]` install whose kernels run ONLY in the browser tab gets UNOPTIMIZED
artifacts unless `wasmtime` is importable -- a size/perf regression, never a correctness one (the
unoptimized artifact is valid and `applied: false` is truthful). Stated in `optimizer.error`, in
`pyths doctor`'s optimiser line (`NO_VALIDATOR_DOCTOR_LINE`, M7) and in the README `[server]` line.
Follow-up (v0.2.6): `pyths check --wasm <file>` backed by the compiler's own `wasmparser`.
"""
from __future__ import annotations

import hashlib
import os
import re
import shutil
import subprocess
import tempfile
import time
from dataclasses import asdict, dataclass
from pathlib import Path

__all__ = [
    "WASM_OPT_ENV", "NO_WASM_OPT_NAME", "OPTIMIZER_NONE", "PRESERVED_SECTIONS", "NO_VALIDATOR_ERROR",
    "NO_VALIDATOR_DOCTOR_LINE", "Optimizer", "OptimizerRecord", "resolve_wasm_opt", "optimizer_key",
    "optimize_artifact_wasm", "custom_sections", "is_bare_name", "is_truly_absolute", "search_path",
    "compiler_pass_off_env",
]

WASM_OPT_ENV = "PYTHS_WASM_OPT"
PROGRAM = "wasm-opt"
# The absolute-but-missing override the compiler subprocess is given so ITS pass is off by the
# `resolve_program` contract (refused; never falls back to PATH).
NO_WASM_OPT_NAME = ".no-wasm-opt"
OPTIMIZER_NONE = "none"
# Custom sections the pass must carry across the optimizer round-trip byte-for-byte: the compiler's
# ownership marker (`optimize.rs::GENERATED_SECTION_NAME`) and the M2.1 ABI section.
PRESERVED_SECTIONS = ("pythscribe.generated", "pyths.abi")
# The one sentence the three surfaces (manifest error, doctor line, README) must carry (rev-7 SF-D).
TAB_PATH_SENTENCE = (
    "optimized output is adopted only when a validator is installed: `pip install wasmtime` or "
    "`pythscribe[server]` -- this applies to browser-tab artifacts too"
)
NO_VALIDATOR_ERROR = f"no validator available (pip install pythscribe[server]); optimized output not adopted ({TAB_PATH_SENTENCE})"
NO_VALIDATOR_DOCTOR_LINE = f"optimizer found but no validator: install `pythscribe[server]` to adopt optimized output ({TAB_PATH_SENTENCE})"
# A wall-clock bound on the optimizer subprocess; a hung/killed optimizer is a failed pass (degrade).
OPTIMIZER_TIMEOUT_S = 300.0
_VERSION_RE = re.compile(r"version\s+(\S+)")
_WASM_MAGIC = b"\0asm"


@dataclass(frozen=True)
class Optimizer:
    """The AMBIENT optimizer identity. `id` is `none` or `wasm-opt/<version>`; `path` is the
    resolved binary (None when absent/refused/unprobeable); `error` says why `id` is `none` when a
    binary was found or an override was given but could not be honoured (diagnostic only)."""

    id: str
    path: Path | None = None
    path_sha256: str | None = None
    error: str | None = None

    @property
    def available(self) -> bool:
        return self.path is not None and self.id != OPTIMIZER_NONE


@dataclass(frozen=True)
class OptimizerRecord:
    """The manifest's `optimizer` field: identity + whether the pass actually ran and was adopted."""

    id: str
    path_sha256: str | None
    applied: bool
    error: str | None

    def as_manifest(self) -> dict:
        return asdict(self)


# ----------------------------------------------------------------------------- the resolver (mirror)


def is_bare_name(s: str) -> bool:
    """`procutil::is_bare_name`: exactly one normal path component -- no separator, no drive/UNC/
    verbatim prefix, no root, not `.`/`..`, and on Windows no `:` (drive-relative `C:x` and NTFS ADS
    `prog:ads` are paths, not names)."""
    if not s or s in (".", ".."):
        return False
    if os.name == "nt":
        if ":" in s or "/" in s or "\\" in s:
            return False
        return True
    return "/" not in s


def _is_sep(c: str) -> bool:
    return c == "/" or (os.name == "nt" and c == "\\")


def is_truly_absolute(s: str) -> bool:
    """`std::path::Path::is_absolute` (the Rust resolver's notion). POSIX: starts with `/`. Windows:
    a prefix WITH a root -- `C:\\x` / `C:/x` (disk + root), `\\\\server\\share\\x` (well-formed UNC,
    either separator, non-empty server, ONE separator, non-empty share; #442), verbatim `\\\\?\\...`
    and device `\\\\.\\...` prefixes (implicit root). NOT absolute: `C:x` (drive-relative), `\\x`
    (rooted-driveless), and every incomplete UNC (`//server`, `//server/`, `//server//x`, `///a/b`)."""
    if os.name != "nt":
        return s.startswith("/")
    if len(s) >= 2 and s[0].isascii() and s[0].isalpha() and s[1] == ":":
        return len(s) >= 3 and _is_sep(s[2])
    if len(s) >= 2 and _is_sep(s[0]) and _is_sep(s[1]):
        rest = s[2:]
        if len(rest) >= 2 and rest[0] in ("?", ".") and _is_sep(rest[1]):
            return True  # verbatim / device-namespace prefix: implicit root
        # UNC: server, exactly one separator, share (both non-empty)
        i = 0
        while i < len(rest) and not _is_sep(rest[i]):
            i += 1
        server = rest[:i]
        if not server or i >= len(rest):
            return False
        j = i + 1
        k = j
        while k < len(rest) and not _is_sep(rest[k]):
            k += 1
        share = rest[j:k]
        return bool(share)
    return False


def _executable_extensions() -> list[str]:
    """`procutil::executable_extensions`: Windows PATHEXT (trimmed, non-empty) with the same fixed
    default; nothing on POSIX."""
    if os.name != "nt":
        return []
    raw = os.environ.get("PATHEXT")
    if raw is None:
        return [".COM", ".EXE", ".BAT", ".CMD"]
    return [e.strip() for e in raw.split(";") if e.strip()]


def _split_path_entries(path_var: str) -> list[str]:
    """`std::env::split_paths`: `;` on Windows (a double-quoted entry is unquoted), `:` elsewhere."""
    out: list[str] = []
    if os.name == "nt":
        for ent in path_var.split(";"):
            if len(ent) >= 2 and ent[0] == '"' and ent[-1] == '"':
                ent = ent[1:-1]
            out.append(ent)
        return out
    return path_var.split(":")


def search_path(name: str) -> Path | None:
    """`procutil::search_path`: a non-bare name is honoured ONLY as a truly absolute existing file
    (it never reaches the PATH loop); a bare name is looked up in the ABSOLUTE PATH directories
    only -- empty, `.` and relative entries are skipped -- as `<dir>/<name>` then `<dir>/<name><ext>`
    per PATHEXT. Every returned path is absolute."""
    if not is_bare_name(name):
        p = Path(name)
        return p if is_truly_absolute(name) and p.is_file() else None
    path_var = os.environ.get("PATH")
    if path_var is None:
        return None
    exts = _executable_extensions()
    for d in _split_path_entries(path_var):
        if not d or d == "." or not is_truly_absolute(d):
            continue
        direct = Path(d) / name
        if direct.is_file():
            return direct
        for ext in exts:
            cand = Path(d) / f"{name}{ext}"
            if cand.is_file():
                return cand
    return None


def resolve_program(name: str, env_override: str | None) -> Path | None:
    """`procutil::resolve_program`, verbatim: a non-empty override that is not bare must be a truly
    absolute existing file (else REFUSED -> None, no fallback); a bare override is PATH-searched;
    no override -> PATH-search `name`."""
    if env_override is not None:
        val = os.environ.get(env_override)
        if val:
            if not is_bare_name(val):
                p = Path(val)
                if is_truly_absolute(val) and p.is_file():
                    return p
                return None
            return search_path(val)
    return search_path(name)


def _probe_version(path: Path) -> tuple[str | None, str | None]:
    """`<path> --version` -> (version, None) or (None, why). The compiler only checks that the probe
    SPAWNS; the Python side needs the version for the identity, so an unparseable/failed probe is
    'not usable' (the build proceeds unoptimized with the reason recorded)."""
    try:
        out = subprocess.run([str(path), "--version"], capture_output=True, text=True, check=False, timeout=30)
    except (OSError, subprocess.SubprocessError) as e:
        return None, f"{PROGRAM} at {path} failed its --version probe ({type(e).__name__}: {e})"
    text = (out.stdout or "") + (out.stderr or "")
    if out.returncode != 0:
        return None, f"{PROGRAM} at {path} failed its --version probe (exit {out.returncode}): {text.strip()[:200]}"
    m = _VERSION_RE.search(text)
    if not m:
        return None, f"{PROGRAM} at {path} printed {text.strip()[:80]!r}, not a `wasm-opt version N` line"
    return m.group(1), None


def _sha256_file(path: Path) -> str | None:
    try:
        h = hashlib.sha256()
        with open(path, "rb") as f:
            for chunk in iter(lambda: f.read(1 << 16), b""):
                h.update(chunk)
        return h.hexdigest()
    except OSError:
        return None


def resolve_wasm_opt() -> Optimizer:
    """The ambient optimizer identity for THIS process/environment (never cached: the environment is
    the input). `id == 'none'` when absent, when the override is refused, or when the binary fails its
    version probe -- `error` says which."""
    override = os.environ.get(WASM_OPT_ENV)
    path = resolve_program(PROGRAM, WASM_OPT_ENV)
    if path is None:
        if override:
            return Optimizer(OPTIMIZER_NONE, error=(
                f"{WASM_OPT_ENV}={override!r} refused: a bare name must be on PATH and a path must be a "
                "truly absolute existing file (a relative, drive-relative or missing override is never "
                "resolved against the cwd and never falls back to PATH)"
            ))
        return Optimizer(OPTIMIZER_NONE)
    version, why = _probe_version(path)
    digest = _sha256_file(path)
    if version is None:
        return Optimizer(OPTIMIZER_NONE, path=None, path_sha256=digest, error=why)
    return Optimizer(f"{PROGRAM}/{version}", path=path, path_sha256=digest)


_KEY_BAD = re.compile(r"[^A-Za-z0-9._-]+")


def optimizer_key(optimizer_id: str) -> str:
    """The optimizer id as ONE filesystem path segment for the JIT cache key
    (`<root>/<COMPILER_VERSION>/<optimizer key>/<source-sha>`). `none` stays `none`."""
    key = _KEY_BAD.sub("_", optimizer_id).strip("._") or "unknown"
    return key[:64]


def compiler_pass_off_env(adir: Path) -> dict[str, str]:
    """The environment `pyths compile` runs under so the compiler's OWN wasm-opt pass is off by the
    `resolve_program` contract: an absolute path that does not exist is refused, never PATH-fallen-back."""
    return {**os.environ, WASM_OPT_ENV: str(Path(adir).resolve() / NO_WASM_OPT_NAME)}


# ----------------------------------------------------------------------------- module inspection


class _Malformed(ValueError):
    pass


def _read_leb128_u32(b: bytes, pos: int) -> tuple[int, int]:
    result = 0
    shift = 0
    for i in range(5):
        if pos + i >= len(b):
            raise _Malformed("truncated LEB128")
        byte = b[pos + i]
        result |= (byte & 0x7F) << shift
        if byte & 0x80 == 0:
            return result, pos + i + 1
        shift += 7
    raise _Malformed("oversized LEB128")


def _leb128_u32(v: int) -> bytes:
    out = bytearray()
    while True:
        byte = v & 0x7F
        v >>= 7
        if v == 0:
            out.append(byte)
            return bytes(out)
        out.append(byte | 0x80)


def custom_sections(b: bytes) -> list[tuple[str, bytes]]:
    """Every custom section as (name, RAW encoded section bytes: id + size + payload), in order. A
    STRUCTURAL walk only (mirrors `optimize.rs::has_generated_marker`, fail-closed on malformed
    input) -- it is a diagnostic and a re-append helper, never a validator."""
    if len(b) < 8 or b[:4] != _WASM_MAGIC:
        raise _Malformed("not a WebAssembly module (bad magic)")
    out: list[tuple[str, bytes]] = []
    i = 8
    while i < len(b):
        start = i
        sid = b[i]
        i += 1
        size, i = _read_leb128_u32(b, i)
        end = i + size
        if end > len(b):
            raise _Malformed("section overruns the module")
        if sid == 0:
            n, j = _read_leb128_u32(b, i)
            if j + n > end:
                raise _Malformed("custom section name overruns the section")
            try:
                name = b[j : j + n].decode("utf-8")
            except UnicodeDecodeError as e:
                raise _Malformed("custom section name is not UTF-8") from e
            out.append((name, b[start:end]))
        i = end
    return out


def encode_custom_section(name: str, contents: bytes) -> bytes:
    nb = name.encode("utf-8")
    payload = _leb128_u32(len(nb)) + nb + contents
    return b"\0" + _leb128_u32(len(payload)) + payload


# ----------------------------------------------------------------------------- the publish gate


def _load_validator():
    """The ONLY publish authority: `wasmtime.Module.validate` on the runtime's own engine. Returns
    a callable(bytes) that raises on an invalid module, or None when wasmtime is not importable."""
    try:
        import wasmtime  # noqa: F401
    except ImportError:
        return None
    from ..runtime import _engine

    engine = _engine(False)

    def validate(final: bytes) -> None:
        wasmtime.Module.validate(engine, final)

    return validate


def _rmtree_retry(d: Path) -> bool:
    for _ in range(5):
        shutil.rmtree(d, ignore_errors=True)
        if not d.exists():
            return True
        time.sleep(0.2)
    return not d.exists()


class _Refused(Exception):
    """The candidate is not adopted; the message is the manifest's `optimizer.error`."""


def _assemble_candidate(original: bytes, out_path: Path) -> bytes:
    """(a) exit 0 already checked; (b) size > 0; (c) every preserved custom section present and
    byte-equal to the input's, re-appended into the candidate if the optimizer stripped it. Returns
    the FINAL candidate bytes (what the validator sees and what is published)."""
    try:
        candidate = out_path.read_bytes()
    except OSError as e:
        raise _Refused(f"{PROGRAM} produced no readable output ({e})") from e
    if not candidate:
        raise _Refused(f"{PROGRAM} produced an empty output")
    try:
        in_sections = dict(custom_sections(original))
    except _Malformed as e:
        raise _Refused(f"the compiled module is malformed before optimization ({e}); not optimized") from e
    try:
        out_sections = dict(custom_sections(candidate))
    except _Malformed as e:
        raise _Refused(f"{PROGRAM} output is not a well-formed WebAssembly module ({e})") from e
    final = bytearray(candidate)
    for name in PRESERVED_SECTIONS:
        raw = in_sections.get(name)
        if raw is None:
            continue
        got = out_sections.get(name)
        if got is None:
            final += raw  # re-append INTO the tmp candidate, exactly as optimize.rs does
        elif got != raw:
            raise _Refused(f"{PROGRAM} altered the `{name}` custom section; output refused")
    return bytes(final)


def optimize_artifact_wasm(wasm_path: Path, adir: Path, optimizer: Optimizer) -> OptimizerRecord:
    """The Python-owned pass. NEVER in place, NEVER raises: the original `<name>.wasm` is untouched
    unless the FINAL candidate passed the wasmtime publish gate, and then it is replaced by ONE
    atomic `os.replace`. Every other outcome keeps the original and records `applied: false` +
    the reason. Must run BEFORE the manifest/hash step (the pass is never a post-manifest mutation)."""
    wasm_path = Path(wasm_path)
    adir = Path(adir)
    if not optimizer.available:
        return OptimizerRecord(OPTIMIZER_NONE, optimizer.path_sha256, False, optimizer.error)
    record_id, digest = optimizer.id, optimizer.path_sha256
    tmp: Path | None = None
    try:
        original = wasm_path.read_bytes()
        tmp = Path(tempfile.mkdtemp(prefix=".pyths-opt-", dir=str(adir)))
        out = tmp / "out.wasm"
        cmd = [str(optimizer.path), "-Os", str(wasm_path), "-o", str(out)]
        try:
            proc = subprocess.run(cmd, capture_output=True, text=True, check=False, timeout=OPTIMIZER_TIMEOUT_S)
        except subprocess.TimeoutExpired:
            raise _Refused(f"{PROGRAM} timed out after {OPTIMIZER_TIMEOUT_S:g}s and was killed; output discarded") from None
        except (OSError, subprocess.SubprocessError) as e:
            raise _Refused(f"{PROGRAM} did not run ({type(e).__name__}: {e})") from e
        if proc.returncode != 0:
            raise _Refused(f"{PROGRAM} failed (exit {proc.returncode}): {(proc.stderr or proc.stdout).strip()[:300]}")
        final = _assemble_candidate(original, out)
        validate = _load_validator()
        if validate is None:
            raise _Refused(NO_VALIDATOR_ERROR)
        try:
            validate(final)  # on the FINAL bytes, i.e. AFTER the re-append (rev-6 B1 / F8g)
        except Exception as e:  # noqa: BLE001 -- wasmtime's error type; any failure = not adopted
            raise _Refused(f"optimized module failed validation ({type(e).__name__}: {str(e).splitlines()[0][:200]})") from e
        staged = tmp / "final.wasm"
        staged.write_bytes(final)
        os.replace(str(staged), str(wasm_path))  # the ONE mutation of the artifact; atomic
        return OptimizerRecord(record_id, digest, True, None)
    except _Refused as e:
        return OptimizerRecord(record_id, digest, False, str(e))
    except Exception as e:  # noqa: BLE001 -- the pass degrades, it never fails a build
        return OptimizerRecord(record_id, digest, False, f"optimizer pass aborted ({type(e).__name__}: {e})")
    finally:
        if tmp is not None and tmp.exists() and not _rmtree_retry(tmp):
            # a lingering tmp dir would be swept into the manifest's file list; make it loud
            raise RuntimeError(f"could not remove the optimizer temp dir {tmp}; remove it and rebuild")
