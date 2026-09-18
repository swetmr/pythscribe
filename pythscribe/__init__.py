"""pythscribe -- `@wasm` for ordinary Python, with a load-bearing Python fallback.

    from pythscribe import wasm

    @wasm
    def edit_distance(a: list[int], b: list[int], prev: list[int], cur: list[int]) -> int:
        ...

`@wasm` never compiles anything at import time. It (1) statically checks that the
decoration is readable (a literal, unconditional, top-level `@wasm`), (2) looks for a
pre-built artifact laid beside the source by `python -m pythscribe.build`, and (3) resolves
ONE of three modes for this process -- `server` (the artifact's `.wasm` runs in-process under
wasmtime, sandboxed), `browser` (a framework adapter runs it in the user's tab; direct calls
run Python), or `fallback` (plain Python) -- and returns a wrapper. The resolved binding is
`fn.__pythscribe__` (`binding_of(fn)`): `mode`, `mode_reason`, the artifact, and the two
server-side path markers `python_calls` / `server_calls`.
"""
from ._pin import COMPILER_COMMIT, COMPILER_VERSION
from .artifacts import ArtifactError, ArtifactInfo, ArtifactNotFoundError, ArtifactWarning
from .decorators import MODES, ModeError, StaticDecorationError, WasmBinding, binding_of, wasm

__version__ = "0.2.8"

__all__ = [
    "wasm",
    "binding_of",
    "WasmBinding",
    "StaticDecorationError",
    "ModeError",
    "MODES",
    "ArtifactInfo",
    "ArtifactError",
    "ArtifactNotFoundError",
    "ArtifactWarning",
    "COMPILER_VERSION",
    "COMPILER_COMMIT",
    "__version__",
]
