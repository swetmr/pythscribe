"""v0.2.5 Gradio callback path -- the public Python surface for marking a `@wasm` function as a
*synchronous* client-side callback and attaching it to a host widget (a ReactFlow custom node's
compute, a data-grid comparator / cell renderer).

`client_callback(fn, shape=...)` validates the kernel's signature against the shape's contract,
compiles its `.wasm` (compile-on-first-call, graceful), and returns a **TRANSPORT-NEUTRAL**
`CallbackSpec` whose `.to_payload()` is a **plain JSON dict** -- so the vendored `gradio_wasmfunction`
backend never has to import `pythscribe` (S-r3-2). The spec crosses to the frontend, which
instantiates the `.wasm` ONCE and hands the kernel to the widget as a synchronous JS callback (no
per-call server round-trip).

**Transport neutrality (B2).** `client_callback` does NOT compute a served URL and does NOT
`import gradio`: it records only the artifact's `.wasm` PATH (`wasm_path`) and leaves `wasm=None`.
The HOST attaches the concrete transport at postprocess time -- the Gradio `FlowGraph`/`CallbackGrid`
turns `wasm_path` into its served static-file URL (`/gradio_api/file=...`), the future Streamlit host
will attach a base64 blob instead. This is what lets `pip install pythscribe[streamlit]` (no gradio on
`sys.path`) import the callback surface and call `client_callback` on a compiled kernel without
crashing: nothing gradio-specific runs until a Gradio *host* attaches its transport.

Return authority is PER SHAPE (S2) -- this path does NOT cross the JSON `bits` return of the
preprocessing component, so `_require_scalar_float_return` (#495, float-only) does NOT apply; a
legal `-> int` comparator/cell is ADMITTED:

    node        (scalars) -> {float, int}   the flagship ReactFlow custom-node compute
    comparator  (a, b)    -> {int}          drives Array.prototype.sort
    cell        (value)   -> {float, int}   a numeric cell renderer (bar / badge)

Params are restricted to `int` | `float` (B5c): `bool` (would coerce `x?1:0`, silently diverging
from a CPython `2.5`-valued bool arg), lists, and typed arrays are refused at `client_callback`.
"""
from __future__ import annotations

from dataclasses import dataclass
from typing import Any, Callable

from ..decorators import binding_of
from ..ffi import signature_of

# per-shape contract: allowed param arity (None = any >=1) and the allowed return types (S2)
_SHAPES: dict[str, dict[str, Any]] = {
    "node": {"arity": None, "returns": {"float", "int"}},
    "comparator": {"arity": 2, "returns": {"int"}},
    "cell": {"arity": 1, "returns": {"float", "int"}},
}
_SCALAR_PARAMS = {"int", "float"}  # bool/list/array refused (B5c)
_RENDERS = {"bar", "badge"}


@dataclass(frozen=True)
class CallbackSpec:
    """A `@wasm` kernel serialized as a TRANSPORT-NEUTRAL synchronous client-side callback.
    `.to_payload()` is a plain JSON dict (no pythscribe types) that the vendored backend forwards to
    the frontend.

    `wasm` is the concrete transport URL the HOST attaches (`None` here -- `client_callback` never
    computes it, so no `import gradio`); `wasm_path` is the neutral artifact `.wasm` path the host
    resolves into that transport (a served URL for Gradio, a base64 blob for Streamlit). The frontend
    reads only `wasm` (host-attached); `wasm_path` is a server-side field the host consumes and strips
    before the payload reaches the client. A host MUST derive the transport ONLY from this server-side
    object (B-2): never from a `wasm_path` found in a client-returned component value -- for Gradio
    that would register an attacker-named directory as a static path (arbitrary file read); for
    Streamlit, base64-ing a client-named path would be a direct file read (SF-D)."""

    fn: str
    wasm: str | None
    param_types: list[str]
    return_type: str
    shape: str
    source_sha256: str
    render: str | None = None
    wasm_path: str | None = None

    def to_payload(self) -> dict[str, Any]:
        p: dict[str, Any] = {
            "fn": self.fn,
            "wasm": self.wasm,  # None from client_callback; the HOST attaches the transport (B2)
            "param_types": list(self.param_types),
            "return_type": self.return_type,
            "shape": self.shape,
            "source_sha256": self.source_sha256,
        }
        if self.render is not None:
            p["render"] = self.render
        if self.wasm_path is not None:
            p["wasm_path"] = self.wasm_path  # neutral artifact path; the host turns it into `wasm`
        return p


def client_callback(fn: Callable[..., Any], shape: str = "node", *, render: str | None = None) -> CallbackSpec:
    """Mark `@wasm` `fn` as a synchronous client-side callback of the given `shape`.

    Validates the signature against the shape's contract (per-shape return authority, S2; scalar
    `int`/`float` params only, B5c), compiles on first call (graceful), and returns a TRANSPORT-NEUTRAL
    `CallbackSpec` (`wasm=None`; `wasm_path` carries the artifact path for the host to resolve, B2).
    NO `import gradio`, NO eager URL -- so this works with gradio NOT on `sys.path`. A kernel with no
    usable artifact is REFUSED here with a `RuntimeError` naming the build step (spec §6, SF-B): a
    *callback* cannot fall back to the server (it must be synchronous), so the refusal is at
    app-build time, not an error node in the tab.

    Trust (B-2): `wasm_path` is a SERVER-side field. The Gradio host records it as servable ONLY when
    it serializes this `CallbackSpec` OBJECT (`_kernel_payload`); a `wasm_path` arriving in a
    client-returned payload is stripped and never served (`wasm=None`, loud in the tab)."""
    if shape not in _SHAPES:
        raise ValueError(f"client_callback: unknown shape {shape!r} (expected one of {sorted(_SHAPES)})")
    contract = _SHAPES[shape]

    b = binding_of(fn)
    b.ensure_compiled()  # compile-on-first-call; never raises (graceful, like the image/scalar path)
    params, ret = signature_of(b)

    # param admission (B5c): int|float only -- refuse bool/list/array/typed-array
    for i, p in enumerate(params):
        if p not in _SCALAR_PARAMS:
            raise TypeError(
                f"client_callback({b.name!r}, shape={shape!r}): parameter {i} is {p!r}; only int|float "
                f"scalar params cross a callback boundary (bool/list/array are refused, B5c)"
            )
    if contract["arity"] is not None and len(params) != contract["arity"]:
        raise TypeError(
            f"client_callback({b.name!r}, shape={shape!r}): expected {contract['arity']} param(s), got {len(params)} {params}"
        )
    if not params:
        raise TypeError(f"client_callback({b.name!r}, shape={shape!r}): a callback needs at least one parameter")
    # per-shape return authority (S2): NOT the JSON-bits float-only rule
    if ret not in contract["returns"]:
        raise TypeError(
            f"client_callback({b.name!r}, shape={shape!r}): return type {ret!r} is not allowed for this shape "
            f"(allowed: {sorted(contract['returns'])})"
        )
    if render is not None:
        if shape != "cell":
            raise ValueError(f"client_callback: render={render!r} is only valid for shape='cell'")
        if render not in _RENDERS:
            raise ValueError(f"client_callback: render must be one of {sorted(_RENDERS)}, got {render!r}")

    # SF-B (spec §6): NO usable artifact -> refused HERE, at app-build time, with a clear Python error
    # naming the build step (like the scalar/image clients). A synchronous client callback has no
    # server fallback by design, so a silent `wasm_path=None` would only surface as an error node in
    # the tab. This is the SERVER-build arm; the B-2 CLIENT-echo arm (an untrusted `wasm_path` in a
    # client-returned payload) is deliberately silent->`wasm=None` in the host, never a raise.
    if b.artifact is None:
        raise RuntimeError(
            f"client_callback({b.name!r}, shape={shape!r}): no usable artifact ({b.artifact_status}); build it "
            f"(`python -m pythscribe.build <module.py>`, or leave compile-on-first-call ON: unset PYTHSCRIBE_NO_JIT) "
            f"-- a synchronous client callback has no server fallback by design (spec §6)"
        )
    # B2: record only the artifact PATH (transport-neutral); the host attaches the transport. No
    # `wasm_url(fn)` here -- that would `import gradio` + register a static path eagerly, crashing a
    # gradio-free install (e.g. pythscribe[streamlit]) the moment a compiled artifact exists.
    wasm_path = str(b.artifact.wasm)
    return CallbackSpec(
        fn=b.name,
        wasm=None,
        param_types=params,
        return_type=ret,
        shape=shape,
        source_sha256=b.source_sha256,
        render=render,
        wasm_path=wasm_path,
    )
