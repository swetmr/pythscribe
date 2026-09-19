"""`@wasm` -- the runtime path with the load-bearing no-op Python fallback (plan §5) and,
since M1.5, the in-process SERVER path.

What the decorator does at import time, in order:

1. STATIC CHECK (`_static.locate`): the decoration must be a literal, unconditional,
   top-level `@wasm` bound to pythscribe's decorator. Anything else raises
   StaticDecorationError. This is the gate that prevents a silently-unrouted function.
   If the module's SOURCE is unavailable (sourceless deployment) there is nothing to
   check or build: the function falls back to plain Python with a logged warning
   (status 'unreadable-source') -- a deployed app must not die at import.
2. ARTIFACT LOOKUP (`artifacts.resolve`): a pre-built bundle beside the source whose
   manifest verifies against the CURRENT kernel source. Absent -> fallback, silently.
   Present-but-unusable -> fallback + logged warning. Explicit `artifact=` that is
   unusable -> ArtifactNotFoundError (never a silent no-op).
3. MODE RESOLUTION (`resolve_mode`, ONE authority, coerced ONCE here): which of the three
   modes this process runs the kernel in --
     server   = the artifact's `.wasm` in-process under wasmtime (`pythscribe.runtime`);
                a direct Python call runs WASM; adapters may still send it to the browser
     browser  = artifact bound, but the server path is unavailable here (wasmtime not
                installed / signature outside the FFI grammar): a direct call runs the
                Python body; the Gradio adapter runs the compiled kernel in the tab
     fallback = no usable artifact (or PYTHSCRIBE_MODE=fallback / artifacts disabled):
                plain Python everywhere
   Default `auto` picks the first available in that order. An EXPLICIT request
   (`PYTHSCRIBE_MODE=server|browser|fallback` or `@wasm(mode=...)`) that cannot be honoured
   raises ModeError at import -- a mode is never silently mis-selected; `binding.mode` and
   `binding.mode_reason` say what was chosen and why.
4. WRAP: return a wrapper that runs the server path or the plain Python function per the
   resolved mode. Nothing is compiled at IMPORT, ever (plan §6). But the FIRST CALL of an
   `auto`-mode kernel with no pre-built artifact triggers COMPILE-ON-FIRST-CALL
   (`WasmBinding.ensure_compiled`): the statically-checked source is compiled via the pyths
   compiler into a per-user, source-hash-keyed cache and the server path is bound -- so `@wasm`
   works with no explicit `python -m pythscribe.build` step. It is the SAME artifact/ABI the
   explicit build produces; only WHEN it is produced differs. Any failure (no compiler, not
   WASM-eligible, wasmtime missing, cache unusable) leaves the plain-Python fallback in place,
   never raising. Opt out with PYTHSCRIBE_NO_JIT=1 (prebuilt-only / no runtime compiler);
   override the cache location with PYTHSCRIBE_CACHE.

Path markers (behaviour parity must never mask WHICH path executed): `python_calls` counts
(under a lock) how often the PYTHON BODY ran -- unchanged from M0/M1 -- and `server_calls`
how often the in-process WASM ran. The server path preserves the Python semantics of
in-place list mutation: every list parameter the kernel assigns into is read back from
WASM memory into the caller's list (`runtime.mutated_list_params`, static).

FFI note (plan §7 watch item): the browser adapter crosses exactly one return type, `float`
(as its IEEE-754 bit pattern). The server path returns int/float/bool/None exactly (an i64
that overflowed inside the kernel is REFUSED with OverflowError, never wrapped or re-run).
"""
from __future__ import annotations

import functools
import inspect
import logging
import os
import shutil
import tempfile
import threading
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any, Callable

from ._pin import COMPILER_VERSION
from ._static import Located, SourceUnavailable, StaticDecorationError, locate, positional_only_signature, signature_texts
from .artifacts import MANIFEST_NAME, ArtifactError, ArtifactInfo, ArtifactNotFoundError, loadability_problem, resolve, verify

__all__ = ["wasm", "binding_of", "WasmBinding", "StaticDecorationError", "ModeError", "MODES", "MODE_ENV", "FUEL_ENV", "NO_JIT_ENV"]

log = logging.getLogger("pythscribe")

MODES = ("server", "browser", "fallback")
MODE_ENV = "PYTHSCRIBE_MODE"
FUEL_ENV = "PYTHSCRIBE_FUEL"
NO_JIT_ENV = "PYTHSCRIBE_NO_JIT"  # opt out of compile-on-first-call (prebuilt-only / no runtime compiler)


class ModeError(RuntimeError):
    """An explicitly requested `@wasm` mode cannot be honoured in this process."""


@dataclass
class WasmBinding:
    name: str
    source_file: Path
    kernel_source: str | None
    source_sha256: str | None
    artifact: ArtifactInfo | None
    artifact_status: str  # 'resolved' | 'absent' | 'disabled' | 'unusable' | 'unreadable-source'
    python_fn: Callable[..., Any]
    python_calls: int = 0
    # (name, annotation text) per parameter and the return annotation text, read from the
    # SAME statically-checked source the build compiles (M1: the FFI shim marshals by these;
    # one source of truth for what crosses the boundary). None when the source is unreadable.
    params: tuple[tuple[str, str], ...] | None = None
    return_type: str | None = None
    # True iff every parameter is positional with no default (the only shape the FFI shim
    # can call by position); None when the source is unreadable
    positional_only: bool | None = None
    # M1.5 -- the resolved mode (one of MODES), why, and the server-path objects
    mode: str = "fallback"
    mode_reason: str = ""
    requested_mode: str = "auto"
    fuel: int | None = None
    server: Any = None  # pythscribe.runtime.ServerKernel when mode == 'server'
    mutated_params: frozenset[str] = frozenset()
    list_use_problem: str | None = None  # why the server path cannot honour this kernel's list mutation (None = fine)
    server_calls: int = 0
    _lock: threading.Lock = field(default_factory=threading.Lock, repr=False, compare=False)
    # compile-on-first-call (auto mode, no pre-built artifact): attempted at most once; the
    # reason is kept when it declines/fails so the plain-Python fallback stays explainable.
    _jit_attempted: bool = field(default=False, repr=False, compare=False)
    _jit_reason: str = field(default="", repr=False, compare=False)
    _jit_lock: threading.Lock = field(default_factory=threading.Lock, repr=False, compare=False)

    @property
    def browser_ready(self) -> bool:
        return self.artifact is not None

    @property
    def server_ready(self) -> bool:
        return self.mode == "server" and self.server is not None

    def record_call(self) -> int:
        with self._lock:
            self.python_calls += 1
            return self.python_calls

    def record_server_call(self) -> int:
        with self._lock:
            self.server_calls += 1
            return self.server_calls

    def calls(self) -> int:
        """Python-body runs (the M0/M1 path marker; unchanged meaning)."""
        with self._lock:
            return self.python_calls

    def server_runs(self) -> int:
        with self._lock:
            return self.server_calls

    def counts(self) -> tuple[int, int]:
        """(python_calls, server_calls) read under ONE lock -- the snapshot adapters record at
        dispatch and compare at result time, so a concurrent call can never be attributed to
        one marker and not the other (codex m1.5 r2/#2)."""
        with self._lock:
            return self.python_calls, self.server_calls

    # ------------------------------------------------------------------ the two paths
    def run_python(self, *args: Any, **kwargs: Any) -> Any:
        """The plain Python body, counted. Adapters call this EXPLICITLY for the fallback
        re-run so the `python-fallback` marker keeps its meaning whatever the mode."""
        self.record_call()
        return self.python_fn(*args, **kwargs)

    def run_server(self, *args: Any, **kwargs: Any) -> Any:
        """The in-process WASM path: marshal by the statically-read signature, call the
        export, write mutated lists back into the caller's lists, return the scalar."""
        from .runtime import ServerFfiError, array_param_spec, bind_positional

        if self.server is None or self.params is None or self.return_type is None:
            raise ModeError(f"`{self.name}`: server path not bound (mode={self.mode}: {self.mode_reason})")
        pos = bind_positional(self.python_fn, args, kwargs)
        names = [n for n, _ in self.params]
        types = [t for _, t in self.params]
        read_back = [i for i, n in enumerate(names) if n in self.mutated_params]
        # M2c: a mutated `Array[dtype, ndim]` out-param is a buffer-protocol object (memoryview /
        # np.ndarray / array.array) that `ServerKernel.call` validates and writes back IN PLACE
        # (its `outs[i]` is None); a mutated `list[...]` param is a real list written back here.
        array_out = {i for i in read_back if array_param_spec(types[i]) is not None}
        for i in read_back:  # validated BEFORE the call: the write-back needs a real list/buffer (opus m1.5 r1/S11)
            if i in array_out:
                continue  # ServerKernel.call's total check refuses a non-buffer / wrong-dtype loudly
            if not isinstance(pos[i], list):
                raise ServerFfiError(f"`{self.name}`: parameter `{names[i]}` is mutated by the kernel and must be a list so the result can be written back (got {type(pos[i]).__name__})")
        r = self.server.call(self.name, types, pos, read_back=read_back, return_type=self.return_type, fuel=None)
        self.record_server_call()  # the WASM ran: counted even if the write-back below fails
        for i in read_back:
            if i in array_out:
                continue  # already written into the caller's buffer in place by ServerKernel.call
            pos[i][:] = r.outs[i]  # Python semantics: the caller's list was mutated in place
        return r.value

    # ------------------------------------------------------------- compile-on-first-call
    def ensure_compiled(self) -> None:
        """Zero-config path: if `@wasm` fell back ONLY because no artifact was pre-built, compile
        the (already statically-checked) kernel source now -- once -- into a per-user, source-hash
        keyed cache, and bind the server path. `@wasm` is thereby the whole story: decorate an
        ordinary function and call it; it runs as WASM, no `python -m pythscribe.build`, no
        `import kernels`, no plumbing. Import stays cheap (nothing compiles at import); the one-time
        cost is paid on the FIRST call. ANY failure (no compiler, not WASM-eligible, wasmtime
        missing) leaves the plain-Python fallback in place, logged once -- so a call never crashes
        because a kernel could not be compiled. This is the SAME artifact + server path an explicit
        build produces (one ABI); it only changes WHEN the artifact is produced."""
        if self._jit_attempted or self.mode == "server":
            return
        with self._jit_lock:
            if self._jit_attempted or self.mode == "server":
                return
            self._jit_attempted = True
            # opt-OUT knob (default on): a prebuilt-only / no-compiler-at-runtime deployment, or a
            # test pinning the plain-Python fallback path, sets PYTHSCRIBE_NO_JIT.
            if os.environ.get(NO_JIT_ENV, "").strip() not in ("", "0", "false", "no"):
                self._jit_reason = f"disabled by {NO_JIT_ENV}"
                return
            # JIT is the AUTO path AND fires ONLY when the sole gap is a MISSING artifact. Key on the
            # STATUS, never on `artifact is None`: 'disabled' (PYTHSCRIBE_DISABLE_ARTIFACTS), 'unusable'
            # (a corrupt bundle beside the source) and 'unreadable-source' ALSO have artifact is None,
            # and each is an explicit "stay on Python" that the JIT must not silently override.
            if self.requested_mode != "auto" or self.artifact_status != "absent":
                self._jit_reason = f"not applicable (status={self.artifact_status}, requested={self.requested_mode})"
                return
            if self.kernel_source is None or self.source_sha256 is None:
                self._jit_reason = "kernel source unavailable"
                return
            try:
                from .build import BuildError, build_kernel, find_pyths  # noqa: F401
            except Exception as e:  # pragma: no cover - defensive
                self._jit_reason = f"build support unavailable: {e}"
                return
            try:
                find_pyths()  # the compiler must be present; a pip-only env without it stays fallback
            except BuildError as e:
                self._jit_reason = f"compiler not found; ran as Python ({e})"
                log.info("pythscribe: `%s` running Python fallback (%s)", self.name, self._jit_reason)
                return
            try:
                info = _compile_to_cache(self.name, self.source_sha256, self.kernel_source)
            except BuildError as e:
                self._jit_reason = f"not WASM-eligible; ran as Python ({e})"
                log.info("pythscribe: `%s` compile-on-first-call fell back: %s", self.name, e)
                return
            except Exception as e:  # noqa: BLE001 -- compile-on-first-call must NEVER crash a call
                # unwritable/absent cache, no resolvable home, a torn concurrent build, a misconfigured
                # `pyths` (empty --version -> IndexError; non-UTF-8 compiler output -> UnicodeDecodeError),
                # a self-check ArtifactError -- NONE are the kernel's fault; run plain Python. Catch
                # broadly by design so a new failure surface in the toolchain can never escape here.
                self._jit_reason = f"compile cache unavailable; ran as Python ({type(e).__name__}: {e})"
                log.info("pythscribe: `%s` compile-on-first-call fell back: %s", self.name, e)
                return
            self.artifact = info
            self.artifact_status = "resolved"
        # Bind through the ONE authority (resolve_mode) so mode/mode_reason are TRUTHFUL, including
        # the case where the artifact compiled but the server path can't bind here (-> browser).
        resolve_mode(self, self.requested_mode)
        if self.mode == "server":
            self.mode_reason = "auto: compiled on first call"
        else:
            self._jit_reason = f"compiled, but server path did not bind: {self.mode_reason}"
            log.info("pythscribe: `%s` compiled; mode=%s (%s)", self.name, self.mode, self.mode_reason)


def jit_cache_key_dir(sha: str, optimizer_id: str) -> Path:
    """The JIT cache address of a kernel: `<root>/<COMPILER_VERSION>/<optimizer id, one path segment>/
    <source-sha>` (M3). Different optimizer states are DIFFERENT keys; adoption never crosses them."""
    from .build.optimizer import optimizer_key

    return _jit_cache_root() / COMPILER_VERSION / optimizer_key(optimizer_id) / sha


def _compile_to_cache(name: str, sha: str, ksrc: str) -> ArtifactInfo:
    """Compile `ksrc` into the shared per-user cache CONCURRENCY-SAFELY. The ONE authority split that
    closes the whole class (r1/r2/r3): `build_kernel` (which rmtree+rebuilds IN PLACE on any invalid
    manifest) is called ONLY on a PRIVATE temp dir; the SHARED keyed dir is adopted via `verify()` (a
    pure verifier that never builds) and mutated only by a single atomic `os.replace` when it is
    ABSENT. So the shared dir is never rebuilt in place, and there is no evict/TOCTOU window.

    Cache dir keyed by (COMPILER_VERSION, ambient optimizer id, source-sha256) -- M3: a compiler
    upgrade, a cache shared across venvs, or a different optimizer state (none vs `wasm-opt/<ver>`)
    lands on an EMPTY key -> atomic publish; `_valid_at` ALSO checks the manifest's optimizer id
    (defensive) so an entry can never be adopted across optimizer states. Outcomes: (a) a valid
    current artifact is adopted; (b) an absent key is built privately + published atomically (a
    loser of the race adopts the winner); (c) a present-but-INVALID entry (rare same-version
    corruption) is left UNTOUCHED and this process uses its own private build (never a concurrent
    in-place rebuild of the shared dir). Returns the verified ArtifactInfo. Raises BuildError
    (kernel not WASM-eligible) or OSError/ArtifactError (cache unusable) -- both handled by the
    caller (-> plain-Python fallback)."""
    from .build import artifact_is_fresh, build_kernel
    from .build.optimizer import resolve_wasm_opt

    optimizer = resolve_wasm_opt()  # the ambient identity: part of the KEY and of the freshness check
    keyed = jit_cache_key_dir(sha, optimizer.id)
    root = keyed.parent  # <root>/<COMPILER_VERSION>/<optimizer key>: private builds are siblings of the keyed dir
    root.mkdir(parents=True, exist_ok=True)  # OSError/RuntimeError here -> caller runs Python

    def _valid_at(d: Path) -> ArtifactInfo | None:
        """Pure verify (NEVER builds): a valid artifact under `d` that is FRESH for this compiler pin
        AND this optimizer identity -> info, else None. `verify()` itself stays integrity-only."""
        ad = d / "__pythscribe__" / name
        if not (ad / MANIFEST_NAME).is_file():
            return None
        try:
            info = verify(ad, function=name, expected_source_sha256=sha)
        except (ArtifactError, OSError):
            return None
        # M3 freshness (pin + optimizer id + source sha) AND M2.1 ABI loadability -- a cache entry of
        # the SAME pin but a pre-ABI / drifted module (no or mismatched `pyths.abi` section) is INVALID
        # here -> path (c): a private build, never a silent adopt.
        return info if (artifact_is_fresh(info.manifest, optimizer) and loadability_problem(info) is None) else None

    # (a) adopt a valid published artifact -- pure verify, never touches the shared dir with a builder
    info = _valid_at(keyed)
    if info is not None:
        return info

    # build PRIVATELY (build_kernel's in-place rmtree is confined to `tmp`)
    tmp = Path(tempfile.mkdtemp(dir=str(root)))
    keep_tmp = False
    try:
        build_kernel(tmp / "kernel.py", name, ksrc, quiet=True, optimizer=optimizer)  # -> tmp/__pythscribe__/<name>
        try:
            os.replace(str(tmp), str(keyed))  # (b) publish atomically IFF keyed is ABSENT
            renamed = True
        except OSError:
            renamed = False  # keyed occupied (a winner, or a rare present-but-invalid entry)
        if renamed:
            info = _valid_at(keyed)
            if info is not None:
                return info
            raise OSError(f"published artifact failed verification for {name!r} at {keyed}")
        info = _valid_at(keyed)
        if info is not None:
            return info  # adopt the winner (tmp cleaned in finally)
        # (c) present-but-INVALID keyed (rare same-version corruption): use our PRIVATE build; NEVER
        # mutate the shared dir concurrently. A version bump or manual cache clear supersedes it.
        info = _valid_at(tmp)
        if info is None:
            raise OSError(f"compile produced no valid artifact for {name!r}")
        keep_tmp = True
        log.info("pythscribe: `%s` cache entry at %s is invalid; using a private build (clear the cache to re-share)", name, keyed)
        return info
    finally:
        if not keep_tmp and tmp.exists():  # not kept as the live artifact and not renamed away
            shutil.rmtree(tmp, ignore_errors=True)


def _jit_cache_root() -> Path:
    """Where compile-on-first-call lays its per-user artifact cache (override with
    PYTHSCRIBE_CACHE). Keyed by kernel source-sha256 below this root, so the cache is stable
    across processes/notebook restarts and shared by identical kernels."""
    env = os.environ.get("PYTHSCRIBE_CACHE", "").strip()
    if env:
        return Path(env)
    try:
        return Path.home() / ".cache" / "pythscribe" / "jit"
    except RuntimeError:
        # no resolvable home (docker --user with no passwd entry, systemd DynamicUser, locked CI):
        # fall back to the temp dir rather than letting the FIRST CALL raise (never crash a call).
        return Path(tempfile.gettempdir()) / "pythscribe-jit"


def _requested_mode(explicit: str | None) -> str:
    """The decorator argument wins over the environment; both must name a known mode."""
    req = explicit if explicit is not None else os.environ.get(MODE_ENV, "").strip().lower() or "auto"
    if req not in MODES and req != "auto":
        raise ModeError(f"{MODE_ENV}/mode={req!r}: expected one of {MODES} or 'auto'")
    return req


def _requested_fuel(explicit: int | None) -> int | None:
    if explicit is not None:
        if isinstance(explicit, bool) or not isinstance(explicit, int) or explicit <= 0:
            raise ModeError(f"@wasm(fuel=...) must be a positive int, got {explicit!r}")
        return explicit
    raw = os.environ.get(FUEL_ENV, "").strip()
    if not raw:
        return None
    try:
        v = int(raw, 10)
    except ValueError:
        raise ModeError(f"{FUEL_ENV}={raw!r} is not an int") from None
    if v <= 0:
        raise ModeError(f"{FUEL_ENV} must be a positive int, got {v}")
    return v


NO_RUNTIME_WHY = "the wasm runtime (wasmtime) is not available -- it ships with pythscribe; reinstall with `pip install --upgrade pythscribe`"


def _server_capability(binding: WasmBinding) -> tuple[Any, str]:
    """Try to bind the server path. Returns (ServerKernel, '') or (None, why-not).

    ORDER IS LOAD-BEARING: the wasmtime probe runs LAST among the static preconditions (artifact,
    FFI grammar, list use), so a `NO_RUNTIME_WHY` why-not means every OTHER server precondition
    already passed -- wasmtime is the SOLE blocker. `_warn_if_no_runtime` keys on exactly that."""
    from . import runtime
    from .ffi import FfiError, signature_of

    if binding.artifact is None:
        return None, f"no usable artifact ({binding.artifact_status})"
    try:
        signature_of(binding)
    except FfiError as e:
        return None, f"signature outside the FFI grammar: {e}"
    if binding.list_use_problem:
        return None, f"the server path cannot honour this kernel's list use: {binding.list_use_problem}"
    if not runtime.wasmtime_available():
        return None, NO_RUNTIME_WHY
    try:
        sandbox = runtime.Sandbox(fuel=binding.fuel)
        kernel = runtime.ServerKernel.from_artifact(binding.artifact, sandbox=sandbox)
    except runtime.AbiMismatchError as e:
        # M2.1: `resolve()` already classifies a mismatched artifact as unusable, so this is the
        # rare skew between resolve and bind; it is LOUD (warning + the explicit-mode ModeError
        # carries the message), never a silent misread.
        log.warning("pythscribe: `%s` artifact refused by the WASM ABI check (%s)", binding.name, e)
        return None, f"WASM ABI mismatch: {e}"
    except (runtime.SandboxViolation, runtime.WasmTrap, runtime.RuntimeUnavailable, ValueError, OSError) as e:
        # OSError: the artifact's .wasm was deleted between verify and read_bytes (a concurrent
        # cache eviction) -> treat as unbindable and fall back, never let it escape.
        return None, f"cannot load the artifact's .wasm under wasmtime: {e}"
    return kernel, ""


_warned_no_runtime = False
_warn_lock = threading.Lock()  # the once-flag's check-and-set is atomic (a 2-thread probe emitted 2 warnings before)


def _warn_if_no_runtime(binding: "WasmBinding", why_not: str) -> None:
    """LOUD, once-per-process: if `@wasm` degraded to the Python path SOLELY because the wasm
    runtime (wasmtime) is absent, say so -- a silent no-speedup is the footgun that made users
    think `@wasm` was broken. wasmtime is a CORE dependency, so this fires only on a stripped /
    partial install. PRECISE by construction: `why_not` is `NO_RUNTIME_WHY` only when a usable
    artifact is bound AND the signature is inside the FFI grammar AND the list use is honourable
    (`_server_capability` probes wasmtime LAST) -- i.e. the kernel WOULD run server-side and
    reinstalling wasmtime is the fix. A genuine fallback (no artifact, non-compilable kernel, FFI
    grammar, list use) never reaches here, whether or not wasmtime happens to be installed."""
    global _warned_no_runtime
    if why_not != NO_RUNTIME_WHY:
        return  # degraded for some OTHER reason -- reinstalling wasmtime cannot help; stay quiet
    with _warn_lock:
        if _warned_no_runtime:
            return
        _warned_no_runtime = True
    log.warning(
        "pythscribe: the wasm runtime (wasmtime) is not available, so `@wasm` kernels run as plain "
        "Python with NO speedup. wasmtime ships with pythscribe -- reinstall with "
        "`pip install --upgrade pythscribe` (or `pip install wasmtime`). Run `pyths doctor` to check."
    )


def resolve_mode(binding: WasmBinding, requested: str) -> None:
    """ONE authority for the three modes; sets binding.mode / mode_reason / server."""
    binding.requested_mode = requested
    if requested == "fallback":
        binding.mode, binding.mode_reason = "fallback", "requested"
        return
    if requested == "browser":  # no server probe at all: nothing is compiled that will not be used
        if binding.artifact is None:
            raise ModeError(f"`{binding.name}`: mode 'browser' requested but no usable artifact ({binding.artifact_status})")
        binding.mode, binding.mode_reason = "browser", "requested (direct calls run the Python body; adapters run the artifact in the tab)"
        return
    server, why_not = _server_capability(binding)
    if requested == "server":
        if server is None:
            raise ModeError(f"`{binding.name}`: mode 'server' requested but unavailable: {why_not}")
        binding.server, binding.mode_reason, binding.mode = server, "requested", "server"  # server FIRST: a reader seeing mode==server always sees it bound
        return
    # auto
    if server is not None:
        binding.server, binding.mode_reason, binding.mode = server, "auto: artifact + wasmtime + FFI grammar", "server"  # server before mode (free-threaded safe)
    elif binding.artifact is not None:
        binding.mode, binding.mode_reason = "browser", f"auto: artifact bound, server path unavailable ({why_not})"
        _warn_if_no_runtime(binding, why_not)  # ONLY here: an artifact is bound, so wasmtime can be the sole blocker
        log.info("pythscribe: `%s` mode=browser (%s)", binding.name, why_not)
    else:
        # no artifact at all: reinstalling wasmtime cannot change this outcome -> never the runtime warning
        binding.mode, binding.mode_reason = "fallback", f"auto: {why_not}"


def _apply(fn: Callable[..., Any], *, artifact: str | os.PathLike | None, mode: str | None, fuel: int | None) -> Callable[..., Any]:
    if not inspect.isfunction(fn):
        raise StaticDecorationError(f"@wasm expects a plain Python function, got {fn!r}")
    requested = _requested_mode(mode)
    try:
        loc: Located | None = locate(fn)
    except SourceUnavailable as e:
        if artifact is not None:
            raise ArtifactNotFoundError(
                f"`{fn.__name__}`: explicit artifact {artifact!s} cannot be verified: {e}"
            ) from e
        log.warning("pythscribe: %s; running the Python fallback", e)
        loc = None

    if loc is None:
        info, status = None, "unreadable-source"
    elif requested == "fallback":
        # PYTHSCRIBE_MODE=fallback is the SAME authority as PYTHSCRIBE_DISABLE_ARTIFACTS:
        # nothing is bound, so adapters fall back too. An EXPLICIT artifact= must still
        # exist and verify (a decoration that names a target never silently no-ops).
        if artifact is not None:
            resolve(fn.__code__.co_filename, fn.__name__, loc.source_sha256, explicit=artifact)
        info, status = None, "disabled"
    else:
        info, status = resolve(fn.__code__.co_filename, fn.__name__, loc.source_sha256, explicit=artifact)
    sig = signature_texts(loc.node) if loc else (None, None)
    binding = WasmBinding(
        name=fn.__name__,
        source_file=Path(fn.__code__.co_filename),
        kernel_source=loc.kernel_source if loc else None,
        source_sha256=loc.source_sha256 if loc else None,
        artifact=info,
        artifact_status=status,
        python_fn=fn,
        params=sig[0],
        return_type=sig[1],
        positional_only=positional_only_signature(loc.node) if loc else None,
        fuel=_requested_fuel(fuel),
    )
    if loc is not None:
        from .runtime import mutated_list_params, unsupported_list_use

        binding.mutated_params = mutated_list_params(loc.node)
        binding.list_use_problem = unsupported_list_use(loc.node)
    resolve_mode(binding, requested)

    @functools.wraps(fn)
    def wrapper(*args: Any, **kwargs: Any) -> Any:
        # compile-on-first-call: the first time an `auto` kernel with no pre-built artifact is
        # called, try to compile+bind the server path (once). Never touches import time.
        if binding.mode != "server" and not binding._jit_attempted:
            binding.ensure_compiled()
        if binding.mode == "server":
            return binding.run_server(*args, **kwargs)
        return binding.run_python(*args, **kwargs)

    wrapper.__pythscribe__ = binding  # type: ignore[attr-defined]
    return wrapper


def wasm(
    fn: Callable[..., Any] | None = None,
    *,
    artifact: str | os.PathLike | None = None,
    mode: str | None = None,
    fuel: int | None = None,
):
    """Decorate a module-level numeric kernel. `@wasm` and `@wasm(artifact=<dir>, mode=<server|
    browser|fallback>, fuel=<int>)` are the only admitted forms (see `_static`). `fuel` meters
    the server path (untrusted / LLM-generated code): execution past the budget traps. Fuel is
    OPT-IN: without `fuel=` (or PYTHSCRIBE_FUEL) the server path runs UNMETERED, i.e. an
    unbounded loop runs unbounded -- always set a budget for code you did not write."""
    if fn is None:

        def deco(f: Callable[..., Any]) -> Callable[..., Any]:
            return _apply(f, artifact=artifact, mode=mode, fuel=fuel)

        return deco
    return _apply(fn, artifact=artifact, mode=mode, fuel=fuel)


def binding_of(fn: Callable[..., Any]) -> WasmBinding:
    b = getattr(fn, "__pythscribe__", None)
    if not isinstance(b, WasmBinding):
        raise TypeError(f"{fn!r} is not a @wasm function")
    return b
