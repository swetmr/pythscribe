"""Artifact layout + verification (plan §6: explicit build, artifacts beside the source).

Layout, for `def rms_gain` in `/app/kernels.py`:

    /app/__pythscribe__/rms_gain/
        manifest.json          schema, function, source_sha256, compiler pin, target, files{path: sha256},
                               manifest_sha256 (self-hash of the canonical body)
        rms_gain.ps            the kernel source the compiler was fed (provenance)
        rms_gain.js            browser entry (ES module)      <- `entry`
        rms_gain.glue.js       WASM loader + marshalling (import of pyths-runtime rewritten to ./pyths-runtime/)
        rms_gain.wasm          the compiled kernel
        pyths-runtime/         the runtime modules the glue transitively imports (copied, LF, no sourcemaps)

Every byte the browser can load from the directory is listed and hashed; the directory
must contain NOTHING that is not listed (a few well-known desktop/editor noise files
excepted); and the manifest carries a self-hash so an ACCIDENTALLY edited or truncated
manifest is refused (review R1/SF9; this is an integrity guard against mistakes, not an
authenticity guard against a writer who can re-sign -- R2/N-b). All text files are
written with LF so the hashes are identical on every platform/checkout (review R1/B1).

`resolve()` is what `@wasm` calls at import time. It NEVER raises for an absent artifact
(that is the normal un-built state -> Python fallback). A manifest that exists but does not
verify (missing/corrupt/unlisted file, stale source hash, unknown schema, no .wasm, bad
self-hash, unreadable file) is reported as unusable -> fallback + a LOGGED warning. The
notice goes through `logging`, never `warnings`, so `-W error` / `filterwarnings=error`
cannot turn the fallback into an import failure (review R1/B4b). Only an EXPLICITLY named
artifact that is unusable raises (ArtifactNotFoundError): a decoration that names a target
must not silently no-op.
"""
from __future__ import annotations

import hashlib
import json
import logging
import os
from dataclasses import dataclass, field
from pathlib import Path

from ._pin import MANIFEST_SCHEMA

ARTIFACT_DIRNAME = "__pythscribe__"
MANIFEST_NAME = "manifest.json"
DISABLE_ENV = "PYTHSCRIBE_DISABLE_ARTIFACTS"

log = logging.getLogger("pythscribe")


class ArtifactError(RuntimeError):
    """An artifact directory exists but is not usable (the reason is the message)."""


class ArtifactNotFoundError(FileNotFoundError):
    """An explicitly named artifact is absent or unusable."""


class ArtifactWarning(UserWarning):
    """Category name kept for API stability; unusable-artifact notices are LOGGED under the
    `pythscribe` logger (never raised as warnings, see module docstring)."""


@dataclass(frozen=True)
class ArtifactInfo:
    dir: Path
    function: str
    entry: Path  # the browser ES-module entry (`<fn>.js`)
    wasm: Path
    manifest: dict = field(compare=False, repr=False)

    @property
    def source_sha256(self) -> str:
        return self.manifest["source_sha256"]


def sha256_file(path: Path) -> str:
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(1 << 16), b""):
            h.update(chunk)
    return h.hexdigest()


def canonical_manifest_bytes(manifest: dict) -> bytes:
    """The bytes the self-hash covers: every field except `manifest_sha256`, canonical JSON."""
    body = {k: v for k, v in manifest.items() if k != "manifest_sha256"}
    return json.dumps(body, sort_keys=True, separators=(",", ":"), ensure_ascii=True).encode("utf-8")


def manifest_self_hash(manifest: dict) -> str:
    return hashlib.sha256(canonical_manifest_bytes(manifest)).hexdigest()


# Filesystem noise a desktop/editor drops into any directory (R2/S5). Nothing imports these,
# so tolerating them cannot change what the browser executes; anything else unlisted is refused.
_NOISE_NAMES = {".DS_Store", "Thumbs.db", "desktop.ini"}
_NOISE_SUFFIXES = (".swp", ".orig", "~")


def _is_fs_noise(name: str) -> bool:
    return name in _NOISE_NAMES or name.endswith(_NOISE_SUFFIXES)


def artifact_dir_for(source_file: str | os.PathLike, function: str) -> Path:
    return Path(source_file).resolve().parent / ARTIFACT_DIRNAME / function


def load_manifest(artifact_dir: Path) -> dict:
    mpath = artifact_dir / MANIFEST_NAME
    if not mpath.is_file():
        raise ArtifactError(f"no {MANIFEST_NAME} in {artifact_dir}")
    try:
        manifest = json.loads(mpath.read_text(encoding="utf-8"))
    except (OSError, ValueError) as e:
        raise ArtifactError(f"unreadable manifest {mpath}: {e}") from e
    if not isinstance(manifest, dict):
        raise ArtifactError(f"manifest {mpath} is not an object")
    return manifest


def verify(artifact_dir: Path, *, function: str, expected_source_sha256: str | None) -> ArtifactInfo:
    """Full verification: schema, self-hash, function name, every listed file's sha256, no
    unlisted file in the tree, the entry and the .wasm present, and (if given) the source
    hash matches the CURRENT source. Raises ArtifactError (or OSError for I/O failures).

    INTEGRITY ONLY (M3, spec 13-09-26 blocker #9): this NEVER consults the ambient compiler or
    optimizer, so a valid prebuilt (optimized) artifact is adopted on a machine with no `wasm-opt`
    and no compiler. Freshness (was it built under THIS machine's optimizer identity?) is the
    explicit build's and the JIT cache's concern (`build.artifact_is_fresh`), never this function's."""
    artifact_dir = Path(artifact_dir)
    manifest = load_manifest(artifact_dir)
    if manifest.get("schema") != MANIFEST_SCHEMA:
        raise ArtifactError(f"manifest schema {manifest.get('schema')!r} != {MANIFEST_SCHEMA}")
    self_hash = manifest.get("manifest_sha256")
    if not isinstance(self_hash, str) or self_hash != manifest_self_hash(manifest):
        raise ArtifactError("manifest self-hash mismatch (edited or truncated manifest; re-run `python -m pythscribe.build --force <module.py>`)")
    if manifest.get("function") != function:
        raise ArtifactError(f"manifest is for `{manifest.get('function')}`, not `{function}`")
    if manifest.get("target") != "js+wasm":
        raise ArtifactError(f"manifest target {manifest.get('target')!r} is not 'js+wasm'")
    files = manifest.get("files")
    if not isinstance(files, dict) or not files:
        raise ArtifactError("manifest lists no files")
    for rel, digest in files.items():
        p = artifact_dir / rel
        if not p.is_file():
            raise ArtifactError(f"listed file missing: {rel}")
        actual = sha256_file(p)
        if actual != digest:
            raise ArtifactError(f"sha256 mismatch for {rel} (corrupt or hand-edited artifact)")
    listed = set(files)
    for p in artifact_dir.rglob("*"):
        if p.is_file():
            rel = p.relative_to(artifact_dir).as_posix()
            if rel != MANIFEST_NAME and rel not in listed and not _is_fs_noise(p.name):
                raise ArtifactError(
                    f"unlisted file in artifact directory: {rel} (re-run `python -m pythscribe.build --force <module.py>`)"
                )
    entry_rel = manifest.get("entry")
    wasm_rel = manifest.get("wasm")
    if not entry_rel or entry_rel not in files:
        raise ArtifactError("manifest entry is missing or unlisted")
    if not wasm_rel or wasm_rel not in files:
        raise ArtifactError("manifest has no .wasm: the kernel was not compiled to WASM")
    src_sha = manifest.get("source_sha256")
    if not isinstance(src_sha, str) or not src_sha:
        raise ArtifactError("manifest has no source_sha256")
    if expected_source_sha256 is not None and src_sha != expected_source_sha256:
        raise ArtifactError(
            "artifact is STALE: the function's source changed since it was built "
            "(re-run `python -m pythscribe.build`)"
        )
    return ArtifactInfo(
        dir=artifact_dir,
        function=function,
        entry=artifact_dir / entry_rel,
        wasm=artifact_dir / wasm_rel,
        manifest=manifest,
    )


def _disabled() -> bool:
    return os.environ.get(DISABLE_ENV, "").strip() not in ("", "0", "false", "no")


def resolve(
    source_file: str | os.PathLike,
    function: str,
    source_sha256: str,
    *,
    explicit: str | os.PathLike | None = None,
) -> tuple[ArtifactInfo | None, str]:
    """Import-time lookup. Returns (artifact-or-None, status) where status is one of
    'resolved' | 'absent' | 'disabled' | 'unusable'. Raises ArtifactNotFoundError only for
    an explicitly named artifact that cannot be used. Never lets an I/O error escape."""
    if _disabled():
        if explicit is not None:
            raise ArtifactNotFoundError(
                f"`{function}`: explicit artifact {explicit!s} requested but {DISABLE_ENV} is set"
            )
        return None, "disabled"

    try:
        adir = Path(explicit).resolve() if explicit is not None else artifact_dir_for(source_file, function)
        present = (adir / MANIFEST_NAME).is_file()
    except OSError as e:
        if explicit is not None:
            raise ArtifactNotFoundError(f"`{function}`: explicit artifact {explicit!s} is unusable: {e}") from e
        log.warning("pythscribe: cannot look up an artifact for `%s` (%s); running the Python fallback", function, e)
        return None, "unusable"
    if not present:
        if explicit is not None:
            raise ArtifactNotFoundError(
                f"`{function}`: explicit artifact {adir} does not exist (no {MANIFEST_NAME}); "
                "build it with `python -m pythscribe.build <module.py>` or drop the `artifact=` argument."
            )
        return None, "absent"
    try:
        info = verify(adir, function=function, expected_source_sha256=source_sha256)
        # M2.1 (spec 13-09-26 §5.6): LOADABILITY, distinct from integrity -- Layer 2 (the manifest's
        # compiler version within ACCEPTED_COMPILER_RANGE) and Layer 1 (the module's own `pyths.abi`
        # section agrees with this runtime). Both are answered by the ONE authority `runtime.abi`;
        # neither consults the ambient compiler / optimizer. Out of range or mismatched -> the SAME
        # unusable classification as a corrupt bundle: auto -> Python fallback + WARNING (never a
        # wrong value), explicit artifact= -> raise.
        problem = loadability_problem(info)
        if problem is not None:
            raise ArtifactError(problem)
    except (ArtifactError, OSError) as e:
        if explicit is not None:
            raise ArtifactNotFoundError(f"`{function}`: explicit artifact {adir} is unusable: {e}") from e
        log.warning("pythscribe: artifact for `%s` at %s is unusable (%s); running the Python fallback", function, adir, e)
        return None, "unusable"
    return info, "resolved"


def loadability_problem(info: ArtifactInfo) -> str | None:
    """M2.1: why THIS runtime cannot load a (verified) artifact -- the manifest's compiler version is
    outside `ACCEPTED_COMPILER_RANGE` (Layer 2) or the `.wasm`'s `pyths.abi` section disagrees with
    the runtime's ABI major / layout strings (Layer 1). None when loadable. Never consults the
    ambient toolchain; used by `resolve()`, the build's up-to-date check and the JIT cache."""
    from .runtime import abi

    version = (info.manifest.get("compiler") or {}).get("version") if isinstance(info.manifest.get("compiler"), dict) else None
    problem = abi.compiler_version_problem(version)
    if problem is not None:
        return problem
    try:
        data = info.wasm.read_bytes()
    except OSError as e:
        return f"cannot read {info.wasm.name}: {e}"
    return abi.module_problem(data, name=info.function)
