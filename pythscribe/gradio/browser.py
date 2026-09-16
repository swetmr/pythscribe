"""pythscribe.gradio.browser -- CLIENT-SIDE `@wasm` kernels through Gradio's built-in `js=`
event hook (v0.2.6). No custom component, no `WasmFunction`, and NO server work for the
computation. Two loaders share one mechanism (`_loader_hook`): the IMAGE loader (fix B) for
typed-array `Array[uint8, 2]` filters, and the SCALAR client (fix A) for float-returning kernels.

IMAGE (fix B):

    from pythscribe.gradio.browser import browser_image_loader_js

    demo.load(None, None, None, js=browser_image_loader_js({"threshold": threshold_lum, "sobel": sobel}))
    slider.change(None, [image, kind, slider], [out_image, status],
                  js="(img, kind, thr) => window.pythscribeImage.filter(img, kind, kind === 'threshold' ? {thr: thr * 3} : {})")

SCALAR (fix A):

    from pythscribe.gradio import client_side, Arg

    cs = client_side({"luhn": luhn_ok, "loan": monthly_payment})       # or client_side(one_kernel)
    demo.load(None, None, None, js=cs.loader_js)                       # ONE hook for every kernel
    card.change(None, [card], [out], js=cs.call_js([Arg.digits], kernel="luhn",
                                                    format="v => v === 0 ? 'VALID card' : 'INVALID (fails Luhn)'"))
    months.change(None, [principal, rate, months], [out],
                  js=cs.call_js([Arg.float, Arg.float, Arg.int], kernel="loan", status=True))

The load hook imports the FFI shim (`pythscribe/ffi/list_buffer.mjs`, from a `blob:` module URL
built from its source text -- no extra static path is exposed) and the in-tab client
(`browser_image.js` / `browser_scalar.js`), and binds `window.<global>` to the kernels' OWN
`.wasm` files (served by `wasm_url`, which registers ONLY the artifact directories as static
paths, exactly as `dispatch_image` does). Per event the handler runs the kernel's WASM export in
the tab through the shim; the server does nothing for the computation.

WHAT IS AND IS NOT CLAIMED. Per slider move / keystroke the server does nothing: the transform
(or the scalar computation), the read-back and the display all happen in the tab (the E2Es assert
0 compute requests and a 0 delta on the kernels' server-side counters). The INPUTS, however, are
Gradio component values: an image preset is served from a `file=` URL and a user upload is POSTed
to the server by `gr.Image` before the tab can decode it; a Textbox / Slider value is component
state that Gradio may hold (its initial value comes from the server; a value is sent to the server
by any OTHER event that lists the component as an input). So "the input never reaches the server"
is NOT a property of this path -- "the computation never leaves the tab" is.

Image kernel contract (read from the kernel's statically-checked signature; refused otherwise):
    img: Array[uint8, 2]   the input, [H, W*3] row-major RGB (role by NAME: `img`; never mutated)
    out: Array[uint8, 2]   filled in place (`out`; MUST be written by the kernel)
    h, w: int              optional: the input's dims, filled by the client   (`h`, `w`)
    oh, ow: int            optional: `out`'s own dims (the handler supplies them; else out=[H, W*3])
    <anything else>: int|float|bool  the handler's `extras` -- pass them BY NAME (`{thr: 384}`);
                           a positional array is also accepted (declaration order)
    -> int                 a scalar (the pixel count); the pixels come back through `out`
The six role names `img out h w oh ow` are RESERVED on this contract: a kernel that wants one
of them as a tunable cannot use this loader (refused when typed differently; a same-typed
collision is by construction the role, never an extra).

Scalar kernel contract (fix A; `scalar_kernel_spec`):
    params: int | float | bool | list[int] | list[float]   each produced by one `Arg` of `call_js`
    -> float               the ONE crossed scalar return type (M0; `_require_scalar_float_return`,
                           the same authority `dispatch` uses -- an `int`/`bool`/`None` return is refused)
    NO list parameter may be written by the kernel (`out[i] = ...`): nothing is read back on this
    path, so a mutation CPython's caller would see is refused rather than silently lost. Typed-array
    parameters are the image contract's; they are refused here.
`Arg`s map the handler's Gradio inputs onto the kernel's parameters IN DECLARATION ORDER, one `Arg`
per parameter; every `Arg` but `Arg.const(...)` consumes the next input. The built-in transforms
(`Arg.float`, `Arg.int` -- a non-integer is REFUSED, never truncated --, `Arg.bool`, `Arg.digits`,
`Arg.utf8_bytes`, `Arg.codepoints`, `Arg.floats`, `Arg.ints`) are implemented ONCE, in
`browser_scalar.js`; only `Arg.js("v => ...")` (the raw escape hatch, unchecked) inlines author
JS. The produced type of every checked `Arg` must equal the parameter's type (refused at
app-build time); a const is type-checked against the parameter and must be JSON-exact
(`_check_crossable`). The handler's value is the raw Number (or `format`'s string); with
`status=True` it is `[value, status_line]`. The `window.<global_name>` namespace
(`pythscribeScalar` by default) is RESERVED for the client; several `client_side(...)` objects in
one app compose under it (each hook merges its kernels into the live client; a display name already
bound to a different kernel is refused as a load error, never silently overridden).

There is NO fallback on either path by design: a kernel without a usable artifact is refused at
app-build time (the clients are browser-only), and the shim refuses (throws) rather than
silently re-running on a JS twin, so what ran is always the WASM export. A failure -- of the
load hook itself (e.g. a CSP that blocks `import(blob:)`) or of a call -- is surfaced in the
handler's output / status, never swallowed.

Deployment note: `wasm_url` yields a root-absolute `/gradio_api/file=...` URL; the hook's shared
`resolveUrl` resolves it against Gradio's configured app root (`window.gradio_config.root`) when
present, so a sub-path mount / proxy prefix works as long as Gradio's own file route does.
"""
from __future__ import annotations

import json
from dataclasses import dataclass
from pathlib import Path
from typing import Any, Callable

from ..decorators import WasmBinding, binding_of
from ..ffi import SHIM, array_param_spec, signature_of
from . import _check_crossable, _require_scalar_float_return
from .image import wasm_url

__all__ = ["browser_image_loader_js", "image_kernel_spec", "CLIENT_JS",
           "client_side", "ClientSide", "Arg", "scalar_kernel_spec", "SCALAR_CLIENT_JS"]

CLIENT_JS = Path(__file__).with_name("browser_image.js")
SCALAR_CLIENT_JS = Path(__file__).with_name("browser_scalar.js")
_ROLES = ("img", "out", "h", "w", "oh", "ow")
_ARRAY_T = "Array[uint8, 2]"
_SCALAR_T = ("int", "float", "bool")


def image_kernel_spec(fn: Callable[..., Any]) -> dict[str, Any]:
    """The client's per-kernel spec (see the module docstring): the served .wasm URL, the
    positional param types, the return type, and the role indices. Raises TypeError when the
    signature is outside the image contract, RuntimeError when no artifact is usable."""
    b: WasmBinding = binding_of(fn)
    params, ret = signature_of(b)  # refuses anything outside the FFI grammar / non-positional
    names = [n for n, _ in (b.params or ())]
    if ret != "int":
        raise TypeError(f"`{b.name}`: an image kernel returns `int` (the pixel count; pixels come back via `out`), got {ret!r}")
    roles: dict[str, int] = {}
    extras: list[int] = []
    for i, (name, ann) in enumerate(zip(names, params)):
        norm = ann.replace(" ", "").replace(",", ", ")  # `Array[uint8,2]` -> `Array[uint8, 2]`
        if name in _ROLES:
            want = _ARRAY_T if name in ("img", "out") else "int"
            if norm != want:
                raise TypeError(f"`{b.name}`: parameter `{name}` must be `{want}` in the image contract (`{name}` is a RESERVED role name), got `{ann}`")
            roles[name] = i
        elif norm == _ARRAY_T:
            raise TypeError(f"`{b.name}`: array parameter `{name}` has no role; an image kernel takes exactly `img` and `out` arrays")
        elif norm not in _SCALAR_T:
            raise TypeError(f"`{b.name}`: parameter `{name}: {ann}` is not a scalar; an image kernel's extra parameters are `int`/`float`/`bool` (supplied by the handler)")
        else:
            extras.append(i)
    missing = [r for r in ("img", "out") if r not in roles]
    if missing:
        raise TypeError(f"`{b.name}`: image kernel is missing required parameter(s) {missing} (signature: {', '.join(f'{n}: {a}' for n, a in zip(names, params))})")
    if ("oh" in roles) != ("ow" in roles):
        raise TypeError(f"`{b.name}`: `oh` and `ow` must be given together")
    # the statically-read mutation set (the same authority the server path's write-back uses):
    # only `out` comes back from WASM memory on this path, so a kernel that mutates `img` would
    # silently diverge from CPython / the server path, and one that never writes `out` would
    # silently render black
    if "img" in b.mutated_params:
        raise TypeError(f"`{b.name}`: the image contract never writes `img` back (only `out` is read back from WASM memory); the kernel mutates `img`")
    if "out" not in b.mutated_params:
        raise TypeError(f"`{b.name}`: the kernel never writes `out` (its pixels are the only result channel on this path)")
    b.ensure_compiled()  # contract checked FIRST; then compile-on-first-call (graceful) -- an explicit build is the normal route
    url = wasm_url(fn)
    if url is None:
        raise RuntimeError(f"`{b.name}`: no usable artifact ({b.artifact_status}); build it (`python -m pythscribe.build <module.py>`) -- the in-tab image client has no fallback by design")
    return {
        "fn": b.name,  # the .wasm export name (the display key in `kernels` may differ)
        "wasm": url,
        "param_types": params,
        "return_type": ret,
        "params": names,
        "roles": roles,
        "extras": extras,
        "extra_names": [names[i] for i in extras],
        "source_sha256": b.source_sha256,
    }


# NOTE (S-r2-6/S-r3-1): `wasm_url` is root-absolute (`/gradio_api/file=...`) and is resolved against
# Gradio's configured app root by `resolveUrl`, which is EXPORTED FROM the FFI shim
# (`pythscribe/ffi/list_buffer.mjs`, under the byte-identity gate) -- ONE copy shared by the js-hook
# clients (below) and the callback island. The old inlined `_RESOLVE_URL_JS` Python-string copy (a
# SECOND, un-gated copy of the resolver logic) is DELETED: `_loader_hook` `await import()`s the shim
# before the client source, then aliases `const resolveUrl = ffi.resolveUrl;` so the inlined clients
# keep calling `resolveUrl` bare from the hook closure. No second/inlined resolver copy remains.


def _check_names(where: str, kernels: dict[str, Callable[..., Any]], global_name: str) -> None:
    if not kernels:
        raise ValueError(f"{where}: no kernels")
    if not global_name.isidentifier():
        raise ValueError(f"{where}: global_name {global_name!r} is not a JS identifier")
    for name in kernels:
        if not isinstance(name, str) or not name.replace("-", "_").isidentifier():
            raise ValueError(f"{where}: kernel display name {name!r} must be identifier-like")


def _loader_hook(global_name: str, *, client_src: str, make_call: str, stub_extra: str = "", fail_extra: str = "") -> str:
    """The ONE `js=` load-hook shape both clients use: publish a stub SYNCHRONOUSLY before the
    first `await` (an early handler gets 'loading', not a TypeError; a live client is never
    clobbered by a re-fired hook), import the shim from a `blob:` URL (revoked in `finally`),
    inline the client source, bind `window.<global_name> = <make_call>` and mark it ready; a load
    failure lands in `<global>.loadError` (+ the client-specific `fail_extra` members).

    Escaping: the shim source and the spec cross as `json.dumps` string literals -- ASCII-escaped
    (ensure_ascii), so U+2028/2029 and every non-ASCII character are `\\uXXXX`, and a JSON string
    is a valid JS string literal. Gradio embeds this whole hook (as part of its config) into an
    inline <script> and escapes `< > & '` itself when it does; that is Gradio's invariant, not
    ours -- the invariant to preserve HERE is: nothing is inlined raw except `client_src`, which
    is our own file and must never contain `</script>` (the loader unit tests pin that on the
    EMITTED hook)."""
    shim_src = SHIM.read_text(encoding="utf-8")
    g = f"window.{global_name}"
    stub = f"{{ ready: false, loadError: null{', ' + stub_extra if stub_extra else ''} }}"
    fail = f"{{ ready: false, loadError: msg{', ' + fail_extra if fail_extra else ''} }}"
    return (
        "async () => {\n"
        f"  if (!({g} && {g}.ready)) {g} = {stub};\n"
        "  let __shimUrl = null;\n"
        "  try {\n"
        f"    __shimUrl = URL.createObjectURL(new Blob([{json.dumps(shim_src)}], {{ type: 'text/javascript' }}));\n"
        "    const ffi = await import(__shimUrl);\n"
        "    const resolveUrl = ffi.resolveUrl;\n"
        f"{client_src}\n"
        f"    {g} = {make_call};\n"
        f"    {g}.ready = true;\n"
        "  } catch (e) {\n"
        "    const msg = String(e && e.message ? e.message : e);\n"
        f"    {g} = {fail};\n"
        "  } finally {\n"
        "    if (__shimUrl) URL.revokeObjectURL(__shimUrl);\n"
        "  }\n"
        "}"
    )


def browser_image_loader_js(kernels: dict[str, Callable[..., Any]], *, global_name: str = "pythscribeImage") -> str:
    """The `js=` source for `demo.load(None, None, None, js=...)`: imports the shim + client and
    binds `window.<global_name>` to `kernels` ({display name -> @wasm function}). Every kernel is
    validated and its artifact resolved HERE, at app-build time. A stub with `ready: false` is
    published SYNCHRONOUSLY before the first `await`, so a handler that fires early gets a
    'loading' status instead of a TypeError; a load failure lands in `<global>.loadError` and in
    every subsequent `filter()` status."""
    _check_names("browser_image_loader_js", kernels, global_name)
    spec = {name: image_kernel_spec(fn) for name, fn in kernels.items()}
    return _loader_hook(
        global_name,
        client_src=CLIENT_JS.read_text(encoding="utf-8"),
        make_call=f"makeImageClient(ffi, {json.dumps(spec)})",
        stub_extra="filter: async () => [null, 'loading the in-tab @wasm client...']",
        fail_extra="filter: async () => [null, 'in-tab @wasm client failed to load (NO fallback): ' + msg]",
    )


# ---- fix A: the scalar client -------------------------------------------------------------------

_SCALAR_PARAM_T = ("int", "float", "bool", "list[int]", "list[float]")
_BUILTIN_ARGS: dict[str, str] = {  # transform name (implemented in browser_scalar.js) -> the FFI type it produces
    "float": "float", "int": "int", "bool": "bool",
    "digits": "list[int]", "utf8_bytes": "list[int]", "codepoints": "list[int]",
    "floats": "list[float]", "ints": "list[int]",
}


@dataclass(frozen=True)
class Arg:
    """How ONE kernel parameter is produced from the handler's Gradio inputs (see the module
    docstring). Use the class attributes (`Arg.float`, `Arg.digits`, ...), `Arg.const(value)`
    (consumes no input; type-checked against the parameter) or `Arg.js("v => ...")` (raw JS over
    one input; `produces` may declare the FFI type for the build-time check, else unchecked)."""

    kind: str
    produces: str | None
    consumes: int = 1
    value: Any = None
    js_src: str | None = None

    @classmethod
    def const(cls, value: Any) -> "Arg":
        return cls("const", None, 0, value=value)

    @classmethod
    def js(cls, source: str, *, produces: str | None = None) -> "Arg":
        if not isinstance(source, str) or not source.strip():
            raise ValueError("Arg.js: the source must be a non-empty JS function expression, e.g. \"v => Number(v) * 2\"")
        _refuse_unsafe_js("Arg.js", source)
        if produces is not None and produces not in _SCALAR_PARAM_T:
            raise ValueError(f"Arg.js: produces={produces!r} is not a scalar-client parameter type {_SCALAR_PARAM_T}")
        return cls("js", produces, 1, js_src=source)

    def __repr__(self) -> str:
        if self.kind == "const":
            return f"Arg.const({self.value!r})"
        if self.kind == "js":
            return f"Arg.js({self.js_src!r})"
        return f"Arg.{self.kind}"


for _k, _t in _BUILTIN_ARGS.items():
    setattr(Arg, _k, Arg(_k, _t))
del _k, _t


def _refuse_unsafe_js(where: str, src: str) -> None:
    """Author-supplied JS is the only text inlined RAW into a handler: keep the emitted-handler
    invariants (no `</script`, no raw U+2028/2029) by construction, not by trust."""
    low = src.lower()
    if "</script" in low or "\u2028" in src or "\u2029" in src:
        raise ValueError(f"{where}: the JS source must not contain `</script` or a raw U+2028/U+2029 line separator")


def _const_matches(value: Any, ptype: str) -> bool:
    """A Python const is admitted for a parameter type only when its JSON form is EXACTLY what the
    shim expects for that type (bool is never an int/float here; a float is never an int)."""
    if ptype == "bool":
        return isinstance(value, bool)
    if ptype == "int":
        return isinstance(value, int) and not isinstance(value, bool)
    if ptype == "float":
        return isinstance(value, (int, float)) and not isinstance(value, bool)
    if ptype in ("list[int]", "list[float]"):
        return isinstance(value, (list, tuple)) and all(_const_matches(v, ptype[5:-1]) for v in value)
    return False


def scalar_kernel_spec(fn: Callable[..., Any]) -> dict[str, Any]:
    """The scalar client's per-kernel spec: the served .wasm URL, the positional param types and
    names, the (float) return type. TypeError when the signature is outside the scalar contract,
    RuntimeError when no artifact is usable (never a fallback)."""
    b: WasmBinding = binding_of(fn)
    params, ret = _require_scalar_float_return(b)  # the ONE M0 authority (shared with dispatch): FFI grammar + `-> float`
    names = [n for n, _ in (b.params or ())]
    for name, ann in zip(names, params):
        if array_param_spec(ann) is not None:
            raise TypeError(f"`{b.name}`: parameter `{name}: {ann}` is a typed array; the scalar client marshals scalars and lists only (typed-array kernels use the image loader)")
    if b.mutated_params:
        raise TypeError(f"`{b.name}`: the scalar client reads nothing back, but the kernel writes into {sorted(b.mutated_params)} (`x[i] = ...`); "
                        f"that mutation is visible to CPython's caller and would be silently lost on this path -- refused (use the image loader / the component for out-buffers)")
    b.ensure_compiled()  # contract checked FIRST; then compile-on-first-call (graceful) -- an explicit build is the normal route
    url = wasm_url(fn)
    if url is None:
        raise RuntimeError(f"`{b.name}`: no usable artifact ({b.artifact_status}); build it (`python -m pythscribe.build <module.py>`) -- the in-tab scalar client has no fallback by design")
    return {
        "fn": b.name,  # the .wasm export name (the display key may differ)
        "wasm": url,
        "param_types": params,
        "return_type": ret,
        "params": names,
        "source_sha256": b.source_sha256,
    }


def _is_gradio_component(x: Any) -> bool:
    """True iff `x` is a real Gradio VALUE component (a `gradio.components.base.Component`). The
    named `call_js` form uses this to REFUSE `(Arg, None)` / `(Arg, <arbitrary object>)` -- and a
    layout container like `gr.Row()` (a `BlockContext`, carries no value) -- at app-build time;
    otherwise junk would flow into `inputs=[...]` and only blow up later inside Gradio
    (`AttributeError: 'NoneType' object has no attribute '_id'`, or an empty value) at event wiring.
    Component (not the broader Block) is the right base: only value components are valid event
    inputs."""
    try:
        from gradio.components.base import Component
    except Exception:
        try:  # older/newer layouts may expose it elsewhere; fall back to the value-component base
            from gradio.components import Component  # type: ignore
        except Exception:
            return False  # gradio absent / unrecognised (not this path in practice): refuse, don't guess
    return isinstance(x, Component)


class ClientSide:
    """The result of `client_side(...)`: `loader_js` (ONE `demo.load` hook binding every kernel
    to `window.<global_name>`) and `call_js(...)` (a `js=` handler per event)."""

    def __init__(self, kernels: dict[str, Callable[..., Any]], global_name: str) -> None:
        _check_names("client_side", kernels, global_name)
        self.global_name = global_name
        self.spec: dict[str, dict[str, Any]] = {name: scalar_kernel_spec(fn) for name, fn in kernels.items()}
        # several ClientSide objects may share one global on one page: their hooks COMPOSE (the spec
        # is merged into the live client's; a display name bound to a DIFFERENT kernel is refused)
        self.loader_js: str = _loader_hook(
            global_name,
            client_src=SCALAR_CLIENT_JS.read_text(encoding="utf-8"),
            make_call=f"makeScalarClient(ffi, mergeScalarSpec(window.{global_name}, {json.dumps(self.spec)}))",
        )

    @property
    def kernels(self) -> list[str]:
        return list(self.spec)

    def _pick(self, kernel: str | None) -> str:
        if kernel is None:
            if len(self.spec) != 1:
                raise ValueError(f"client_side.call_js: kernel= is required when several kernels are bound (have: {', '.join(self.spec)})")
            return next(iter(self.spec))
        if kernel not in self.spec:
            raise ValueError(f"client_side.call_js: unknown kernel {kernel!r} (have: {', '.join(self.spec)})")
        return kernel

    def check_args(self, args: list[Arg] | tuple[Arg, ...], kernel: str | None = None) -> str:
        """Validate `args` against the kernel's parameters (one Arg per parameter, produced type ==
        parameter type, consts JSON-exact); returns the kernel's display name."""
        name = self._pick(kernel)
        s = self.spec[name]
        ptypes, pnames = s["param_types"], s["params"]
        if not isinstance(args, (list, tuple)) or not all(isinstance(a, Arg) for a in args):
            raise TypeError(f"client_side.call_js: args must be a list of Arg (one per parameter of `{s['fn']}`: {', '.join(f'{n}: {t}' for n, t in zip(pnames, ptypes))})")
        if len(args) != len(ptypes):
            raise TypeError(f"client_side.call_js: `{s['fn']}` takes {len(ptypes)} parameter(s) ({', '.join(f'{n}: {t}' for n, t in zip(pnames, ptypes))}), got {len(args)} Arg(s)")
        for a, pname, ptype in zip(args, pnames, ptypes):
            if a.kind == "const":
                if not _const_matches(a.value, ptype):
                    raise TypeError(f"client_side.call_js: `{s['fn']}` parameter `{pname}: {ptype}` cannot take the const {a.value!r} (a const must be exactly that type; bool is not a number, a float is not an int)")
                _check_crossable(a.value, f"const for `{pname}`")  # JSON-exact: finite floats, |int| <= 2**53-1
            elif a.produces is not None and a.produces != ptype:
                raise TypeError(f"client_side.call_js: `{s['fn']}` parameter `{pname}: {ptype}` cannot be produced by {a!r} (which yields `{a.produces}`)")
        return name

    def _resolve_named(self, mapping: dict[str, Any], kernel: str | None) -> tuple[str, list[Arg], list[Any]]:
        """Named form: `mapping` binds EVERY parameter by name to an `Arg.const(...)` (no component)
        or a `(Arg, component)` pair. The returned `args` and `inputs` are BOTH in the kernel's
        signature order, so a component can never be bound to the wrong parameter (the positional
        `inputs=[...]` shift class -- closed by construction, matching the image loader's named
        `extras`)."""
        name = self._pick(kernel)
        pnames = self.spec[name]["params"]
        if set(mapping) != set(pnames):
            raise TypeError(f"client_side.call_js: the named mapping must cover EXACTLY the parameters {pnames} of `{self.spec[name]['fn']}`, got {sorted(mapping)}")
        args: list[Arg] = []
        inputs: list[Any] = []
        for pname in pnames:  # SIGNATURE order -- the whole point
            v = mapping[pname]
            if isinstance(v, Arg):
                if v.kind != "const":
                    raise TypeError(f"client_side.call_js: parameter `{pname}` maps to a bare {v!r}; only `Arg.const(...)` takes no component -- a consuming Arg needs a `(Arg, component)` pair")
                args.append(v)
            elif isinstance(v, (tuple, list)) and len(v) == 2 and isinstance(v[0], Arg):
                arg, comp = v
                if arg.kind == "const":
                    raise TypeError(f"client_side.call_js: parameter `{pname}`: `Arg.const(...)` takes no component (drop the component)")
                if not _is_gradio_component(comp):
                    raise TypeError(f"client_side.call_js: parameter `{pname}`: the second element of the pair must be a Gradio value component (a gradio Component, e.g. gr.Number/gr.Slider/gr.Textbox; a layout like gr.Row is not one), got {comp!r} -- a consuming Arg needs the component whose value it transforms")
                args.append(arg)
                inputs.append(comp)
            else:
                raise TypeError(f"client_side.call_js: parameter `{pname}` must map to `Arg.const(...)` or a `(Arg, component)` pair, got {v!r}")
        return name, args, inputs

    def call_js(self, args: "list[Arg] | tuple[Arg, ...] | dict[str, Any]", *, kernel: str | None = None, format: str | None = None, status: bool = False):
        """The `js=` source for an event handler.

        NAMED form (recommended) -- `args` is a `{param_name: Arg.const(...) | (Arg, component)}`
        dict covering every parameter; returns `(js, inputs)` where `inputs` is the component list
        in the kernel's signature order. Pass it straight through:
        `js, inputs = cs.call_js({...}); slider.change(None, inputs, [out], js=js)`. A component is
        bound to its parameter BY NAME, so it cannot be positionally shifted.

        POSITIONAL form (legacy) -- `args` is a list of Arg, one per parameter in signature order;
        returns the `js` string only. Then YOU pass `inputs=[...]` to the event: it must list the
        components the consuming Args take, IN THE SAME ORDER. That order is the author's unchecked
        obligation -- a wrongly-ordered `inputs=[...]` of the right length is a wrong (shifted) call,
        NOT refused. Prefer the named form.

        `outputs=[out]` receives the raw Number, or `format`'s string (`format` = a JS function
        expression `(value, info) => ...`, `info = {kernel, ms, args, bits}`); with `status=True`,
        `outputs=[out, status_box]` receives `[value, status_line]`. Not ready yet -> a 'loading'
        string (or the load failure); a refused input / a trap -> an 'error (in-tab @wasm, NO
        fallback): ...' string and `<global>.lastError` -- never a silent fallback."""
        if isinstance(args, dict):
            name, arglist, inputs = self._resolve_named(args, kernel)
            self.check_args(arglist, name)
            return self._emit_js(arglist, name, format, status), inputs
        name = self.check_args(args, kernel)
        return self._emit_js(args, name, format, status)

    def _emit_js(self, args: "list[Arg] | tuple[Arg, ...]", name: str, format: str | None, status: bool) -> str:
        if format is not None:
            if not isinstance(format, str) or not format.strip():
                raise ValueError("client_side.call_js: format must be a non-empty JS function expression, e.g. \"v => v.toFixed(2) + ' ms'\"")
            _refuse_unsafe_js("client_side.call_js(format=...)", format)
        parts = []
        for a in args:
            if a.kind == "const":
                parts.append(f"{{t: 'const', v: {json.dumps(a.value)}}}")
            elif a.kind == "js":
                parts.append(f"{{t: 'js', f: ({a.js_src})}}")
            else:
                parts.append(f"{{t: {json.dumps(a.kind)}}}")
        g = f"window.{self.global_name}"
        fmt = f"({format})" if format is not None else "null"
        st = "true" if status else "false"
        return (
            "(...a) => {\n"
            f"  const c = {g};\n"
            "  if (!(c && c.ready)) {\n"
            "    const m = c && c.loadError ? 'in-tab @wasm client failed to load (NO fallback): ' + c.loadError : 'loading the in-tab @wasm client...';\n"
            f"    return {st} ? [m, m] : m;\n"
            "  }\n"
            f"  return c.invoke({json.dumps(name)}, a, [{', '.join(parts)}], {fmt}, {st});\n"
            "}"
        )


def client_side(kernels: Callable[..., Any] | dict[str, Callable[..., Any]], *, global_name: str = "pythscribeScalar") -> ClientSide:
    """Bind float-returning `@wasm` kernel(s) to an in-tab client driven from Gradio's `js=`
    hook (see the module docstring). `kernels`: one `@wasm` function (display name = its own) or
    {display name -> function}. Every kernel is validated and its artifact resolved HERE, at
    app-build time; there is no server fallback on this path."""
    if callable(kernels) and not isinstance(kernels, dict):
        kernels = {binding_of(kernels).name: kernels}
    if not isinstance(kernels, dict):
        raise TypeError("client_side: pass a @wasm function or a {name: @wasm function} dict")
    return ClientSide(kernels, global_name)
