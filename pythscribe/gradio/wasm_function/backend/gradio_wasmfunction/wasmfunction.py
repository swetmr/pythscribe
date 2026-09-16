"""WasmFunction -- a Gradio custom component that runs a pythscribe `@wasm` kernel's compiled
bundle in the browser tab and returns the value through the component's value.

The value is a JSON payload (see frontend/types.ts):
    {fn, args, bundle, source_sha256, nonce, result: null | {value, bits, path, ...}, error}
The Python side never interprets the frontend's claims about WHICH path ran without the
server-side `python_calls` counter (see pythscribe.gradio.result_of).
"""
from __future__ import annotations

import threading
from pathlib import Path
from typing import Any
from urllib.parse import quote

from gradio.components.base import Component
from gradio.events import Events

# NB (S-r3-2): this vendored package declares only `gradio` and is installed standalone
# (`pip install -e pythscribe/gradio/wasm_function`); it MUST NOT import `pythscribe`. The callback
# components below therefore accept PLAIN JSON payloads. A kernel object is serialized by DUCK-TYPING
# its `.to_payload()` (a `CallbackSpec` from `pythscribe.gradio.callback`) -- calling a method never
# imports the module, so the direction stays gradio-only.


# --- B2: the HOST attaches the transport -- ONLY for server-authored specs (B-2 trust authority) ---
# A `CallbackSpec` (`pythscribe.gradio.callback`) is TRANSPORT-NEUTRAL: it carries `wasm=None` and a
# neutral `wasm_path` (the artifact `.wasm` path). This Gradio host turns `wasm_path` into its served
# static-file URL at postprocess time and strips the raw server path before the payload reaches the
# client. Mirrors `pythscribe.gradio._static_file_url` -- kept HERE (not imported) so the vendored
# package stays pythscribe-free (S-r3-2); the future Streamlit host attaches a base64 blob instead.
#
# TRUST STATEMENT (B-2, pre-PR review 2026-09-13): the transport is derived ONLY from server-authored
# specs; a client payload can never name a server path. `postprocess` runs on whatever a handler
# returns -- including a value that came in through `preprocess` FROM THE BROWSER (`EVENTS=[change]`,
# so `fg.change(fn, [fg], [fg])` is ordinary wiring) -- and `gr.set_static_paths(dir)` makes EVERY
# file under `dir` readable over the network. So a `wasm_path` is servable iff its RESOLVED path was
# recorded in `_trusted_wasm_paths` by `_kernel_payload`, the ONE place a server-side `CallbackSpec`
# OBJECT (`.to_payload()`-bearing -- cannot arrive from the client, which sends JSON) is serialized.
# Everything else (a client-echoed / hand-built dict naming any path) is stripped and yields
# `wasm: None` -- the island renders its loud "no usable artifact" error; NOTHING is registered.
# The walker is payload-shape-AGNOSTIC (any dict carrying `wasm_path`, at any depth), so every
# callback host built on `_CallbackHost` (FlowGraph now; CallbackGrid's `sort`/`cell` kernels, M1b)
# is covered by construction -- a new host cannot forget to strip.
_registered: set[Path] = set()
_trusted_wasm_paths: set[Path] = set()  # resolved artifact paths, recorded ONLY from server-side CallbackSpec objects
_reg_lock = threading.Lock()


def _resolve_wasm_path(path_str: Any) -> Path | None:
    if not isinstance(path_str, str) or not path_str:
        return None
    try:
        return Path(path_str).resolve()
    except (OSError, RuntimeError, ValueError):
        return None


def _trust_wasm_path(path_str: Any) -> None:
    """Record a SERVER-authored artifact path as servable. Called ONLY from `_kernel_payload` (the
    serialization of a `.to_payload()`-bearing spec object built on the server)."""
    rp = _resolve_wasm_path(path_str)
    if rp is not None:
        with _reg_lock:
            _trusted_wasm_paths.add(rp)


def _is_trusted_wasm_path(rp: Path) -> bool:
    with _reg_lock:
        return rp in _trusted_wasm_paths


def _served_wasm_url(path_str: Any) -> str | None:
    """Return the URL Gradio's file route serves `path_str` under -- ONLY if it is a TRUSTED
    (server-recorded) path that exists; otherwise None, and NO static path is registered (the B-2
    trust check: an untrusted path never reaches `gr.set_static_paths`). Percent-encoded so
    `#`/`?`/`%`/spaces in a checkout path cannot break the frontend's fetch of the `.wasm` URL.

    #506: register the `.wasm` FILE ITSELF as the static path, NOT `rp.parent`. Gradio's
    `set_static_paths([<dir>])` makes EVERY file under `<dir>` readable over the network, and a
    kernel's artifact directory also holds the kernel SOURCE (`*.ps`), `manifest.json`, the
    `*.js`/`*.glue.js` glue, and `pyths-runtime/` -- registering the parent dir on a deployed
    callback app would publish the SOURCE. The callback frontend fetches ONLY the single `.wasm`
    (`list_buffer.mjs::instantiate` -> `fetch(source)` on the `.wasm` URL; the FFI shim is bundled,
    so there is NO relative `./pyths-runtime/*` / `new URL('./x.wasm', import.meta.url)` resolution
    on this path -- unlike the non-callback bundle path `pythscribe.gradio._static_file_url`, which
    serves the whole dir BY NECESSITY for a bundle's relative ES-module imports). Registering the
    file alone (`is_in_or_equal(sibling, <file>)` is False for every sibling) serves exactly what the
    frontend fetches and leaves the source unreadable."""
    rp = _resolve_wasm_path(path_str)
    if rp is None or not _is_trusted_wasm_path(rp):
        return None  # untrusted (client-echoed / hand-built): never registered, never served
    if not rp.is_file():
        return None  # trusted but gone (artifact deleted after build): loud in the tab, not a 404 dir
    import gradio as gr

    with _reg_lock:
        if rp not in _registered:
            gr.set_static_paths([rp])  # the `.wasm` FILE ONLY (#506) -- NOT rp.parent (would expose *.ps/manifest.json)
            _registered.add(rp)
    return "/gradio_api/file=" + quote(rp.as_posix(), safe="/:")


def _attach_transport_to_kernel(spec: Any) -> Any:
    """Resolve one kernel dict's neutral `wasm_path` into a served `wasm` URL; `wasm_path` is ALWAYS
    stripped (a server path must never reach the client). `wasm` is attached ONLY for a trusted path
    (`_served_wasm_url`); an untrusted/absent/missing path leaves `wasm: None`. A spec that already
    carries a `wasm` transport is passed through: the client's own browser fetches that URL under
    Gradio's normal file-route rules, so it cannot widen what the server serves."""
    if not isinstance(spec, dict):
        return spec
    out = {k: v for k, v in spec.items() if k != "wasm_path"}
    if not out.get("wasm"):
        out["wasm"] = _served_wasm_url(spec.get("wasm_path"))  # None unless trusted + existing
    return out


def _attach_transport(value: Any) -> Any:
    """ONE walker for every callback-host payload shape: every dict carrying a `wasm_path` key, at any
    depth (flowgraph `nodes[*].kernel`; grid `sort` / `columns[*].cell`; a future slider spec), goes
    through `_attach_transport_to_kernel`. Lists/dicts are rebuilt; scalars pass through.

    #505: a `.to_payload()`-bearing OBJECT (a `pythscribe.gradio.callback.CallbackSpec`) embedded in a
    handler's return value -- e.g. a raw `{"nodes": [{"kernel": spec}]}` a host built without calling
    `_kernel_payload` -- is routed through `_kernel_payload` (the ONE trust authority), then
    `_attach_transport_to_kernel`. WHY this is sound and does NOT weaken B-2: a `.to_payload()`-bearing
    object can ONLY be server-built (the client sends JSON over the wire, never a live dataclass), so
    `_kernel_payload` correctly GRANTS servability + records the artifact path here, and
    `_attach_transport_to_kernel` then resolves the trusted path to a served `wasm` URL and STRIPS the
    raw `wasm_path`. Plain client dicts stay untrusted (the dict branch below): a dict carrying
    `wasm_path` is stripped and yields `wasm: None`. The object branch is FIRST so the walker never
    falls through to the scalar `return value` (the pre-#505 leak: the dataclass passed through
    untouched, and orjson then serialized its raw `wasm_path` -- the server cache-dir path -- on the
    wire). The walker reaches such an object at ANY depth (bare list element or nested dict value)."""
    if hasattr(value, "to_payload"):
        return _attach_transport_to_kernel(_kernel_payload(value))
    if isinstance(value, dict):
        if "wasm_path" in value:
            return _attach_transport_to_kernel(value)
        return {k: _attach_transport(v) for k, v in value.items()}
    if isinstance(value, list):
        return [_attach_transport(v) for v in value]
    return value


def _attach_flowgraph_transport(value: dict[str, Any]) -> dict[str, Any]:
    """Attach the Gradio transport to every kernel in a flowgraph payload (the shared walker)."""
    return _attach_transport(value)


class WasmFunction(Component):
    EVENTS = [Events.change]

    def preprocess(self, payload: Any) -> Any:
        """The payload as set by the frontend (browser result) or by the script (dispatch)."""
        return payload

    def postprocess(self, value: Any) -> Any:
        """Sent to the frontend unchanged; the frontend computes when `result` is null and `bundle` is set.
        NB: NO transport attachment here (B-2) -- a flowgraph payload routed through a plain
        `WasmFunction` gets no `wasm` and errors loudly in the tab; the only transport authority is
        `_CallbackHost.postprocess` -> `_attach_transport` (trusted, server-recorded paths only)."""
        return value

    def example_payload(self) -> Any:
        return {"fn": "f", "args": [], "bundle": None, "result": None}

    def example_value(self) -> Any:
        return {"fn": "f", "args": [], "bundle": None, "result": None}

    def api_info(self) -> dict[str, Any]:
        return {"type": {}, "description": "pythscribe WasmFunction payload (JSON object)"}


def _kernel_payload(k: Any) -> Any:
    """Serialize a kernel object to plain JSON by DUCK-TYPING `.to_payload()` (a
    `pythscribe.gradio.callback.CallbackSpec`); a dict is passed through. No pythscribe import.

    B-2: this is the ONE place trust is granted. A `.to_payload()`-bearing OBJECT can only be built
    on the server (the client sends JSON), so its `wasm_path` is recorded as servable here, at
    app-build time. A plain dict is NOT trusted (it may be client-derived): its `wasm_path` is
    stripped at postprocess and yields `wasm: None`. Every callback host (FlowGraph; CallbackGrid's
    `sort`/`cell` kernels, M1b) MUST serialize its kernels through this function."""
    if k is None:
        return None
    if hasattr(k, "to_payload"):
        p = k.to_payload()
        if isinstance(p, dict):
            _trust_wasm_path(p.get("wasm_path"))
        return p
    if isinstance(k, dict):
        return k
    raise TypeError(f"callback-host kernel must be a CallbackSpec (with .to_payload()) or a plain dict, got {type(k)!r}")


def _flowgraph_payload(nodes: Any, edges: Any) -> dict[str, Any]:
    out_nodes = []
    for n in nodes or []:
        m = dict(n)
        if "kernel" in m:
            m["kernel"] = _kernel_payload(m["kernel"])
        out_nodes.append(m)
    return {
        "kind": "flowgraph",
        "nodes": out_nodes,
        "edges": [list(e) for e in (edges or [])],
        "nonce": 0,
        "result": None,
        "error": None,
    }


class _CallbackHost(Component):
    """Base of every callback-path host (`FlowGraph`; `CallbackGrid`, M1b). ONE `postprocess`: tag
    the payload `kind` (so the single Index.svelte kind-dispatches, B1) and run the B-2 transport
    walker -- every `wasm_path` at any depth is stripped and `wasm` is attached ONLY for a trusted,
    server-recorded path (`_attach_transport`). A subclass sets `KIND` and serializes its kernels
    through `_kernel_payload`; it MUST NOT override `postprocess` with its own path handling."""

    KIND: str = ""
    EVENTS = [Events.change]

    def preprocess(self, payload: Any) -> Any:
        return payload  # CLIENT data: nothing in it is trusted to name a server path (B-2)

    def postprocess(self, value: Any) -> Any:
        if isinstance(value, dict):
            if self.KIND and value.get("kind") != self.KIND:
                value = {**value, "kind": self.KIND}
            value = _attach_transport(value)
        return value


class FlowGraph(_CallbackHost):
    """A live dataflow graph whose compute nodes run a Python `@wasm` kernel SYNCHRONOUSLY in the
    browser tab (the v0.2.5 callback path). Served by the SAME `templates/component/index.js` as
    `WasmFunction` (kind-dispatch, B1); the React/ReactFlow island is a lazy chunk. Payloads are
    plain JSON (S-r3-2). Transport: `_CallbackHost.postprocess` (B2 attach + B-2 trust)."""

    KIND = "flowgraph"
    EVENTS = [Events.change]

    def __init__(self, nodes: Any = None, edges: Any = None, *, value: Any = None, **kwargs: Any) -> None:
        if value is None and nodes is not None:
            value = _flowgraph_payload(nodes, edges)  # server-side specs -> trust recorded (B-2)
        super().__init__(value=value, **kwargs)

    def example_payload(self) -> Any:
        return {"kind": "flowgraph", "nodes": [], "edges": [], "nonce": 0, "result": None}

    def example_value(self) -> Any:
        return {"kind": "flowgraph", "nodes": [], "edges": [], "nonce": 0, "result": None}

    def api_info(self) -> dict[str, Any]:
        return {"type": {}, "description": "pythscribe FlowGraph payload (JSON object)"}
