"""pythscribe.streamlit -- the Streamlit adapter (v0.2.5 M3): run a `@wasm` kernel's compiled
bundle in the user's browser tab inside a Streamlit custom component (a sandboxed iframe),
return the value to the Streamlit script, and fall back to Python when there is no usable
artifact.

    from pythscribe.streamlit import WasmComponent, dispatch, result_of, describe

    comp = WasmComponent()
    if st.button("Run"):
        st.session_state["payload"] = dispatch(rms_gain, xs, target)
    payload = st.session_state.get("payload")
    r = None
    if payload is not None:                      # nothing dispatched yet on first load
        value = comp(payload=payload, key="rms")
        r = result_of(rms_gain, value, expect_nonce=payload["nonce"])
    st.code(describe(r))

THE IFRAME SEAM (requirements §3.2 -- a MECHANICAL wiring choice, documented): the Gradio
component runs in the main page (same origin) and `fetch()`es the artifact `.wasm` served as a
Gradio static path; a Streamlit component runs in a cross-context sandboxed iframe served by
Streamlit's component server, and the user's artifact is NOT in the vendored component dir. So
this adapter base64-encodes the artifact `.wasm` INTO the component args (`wasm_b64`); the
frontend decodes it and instantiates through the SAME FFI shim (`ffi.instantiate(ArrayBuffer)`,
`how: "bytes"`) the Gradio component uses -- the SAME compiled kernel, the SAME marshalling,
the SAME bit-identical result. Only the transport (inline bytes vs a served URL) differs.

ONE authority for the wire grammar and the path-marker bookkeeping: the framework-neutral
helpers are REUSED from `pythscribe.gradio` (they never import gradio -- that module lazy-imports
its framework, exactly as this one does), so both adapters share ONE nonce/crossable/float-bits/
fallback implementation instead of a divergent copy. The marshalling authority is the ONE FFI
shim (`pythscribe/ffi/list_buffer.mjs`), vendored byte-identically into the component
(`wasm_component/list_buffer.mjs`; identity is a committed gate).

Trust boundary (same discipline as the Gradio adapter): everything the component publishes is
CLIENT data. The server keeps, per dispatch nonce, the arguments it dispatched and the
Python-call counter at dispatch time; `result_of` uses only those (never the client's copy) for
the fallback re-run and for the `python_calls` marker. The float is carried as its IEEE-754 bit
pattern (`bits`) and reconstructed here, so -0.0/inf/nan survive the JSON hop.
"""
from __future__ import annotations

import base64
import collections
import math
import threading
import time
from pathlib import Path
from typing import Any, Callable

from ..decorators import WasmBinding, binding_of
# ONE authority: the framework-neutral dispatch/nonce/marker helpers live in pythscribe.gradio
# (gradio-free at import; the framework is lazy). Reusing them avoids a divergent second copy.
# `_require_scalar_float_return` is the SHARED M0 return-type-admission gate (#495) both adapters
# call, so the "@wasm kernels must return float in M0" contract lives in ONE place.
from ..gradio import (
    _check_crossable,
    _mark_completed,
    _next_nonce,
    _python_result,
    _recall,
    _remember,
    _require_scalar_float_return,
    float_from_bits,
    is_completed,
)
# v0.2.5 callback path. `client_callback` builds the TRANSPORT-NEUTRAL `CallbackSpec` (B2): it
# imports NO gradio and computes NO served URL, so `pip install pythscribe[streamlit]` (gradio
# absent) can call it on a compiled kernel. `slider_compute` (below) is the Streamlit HOST that
# attaches the transport -- base64 bytes from the SERVER-side spec object (SF-D), never a client
# value. Imported from `..gradio.callback` (the module, not the package) so no gradio host code runs.
from ..gradio.callback import CallbackSpec, client_callback

__all__ = [
    "available",
    "require",
    "WasmComponent",
    "dispatch",
    "result_of",
    "deadline_result",
    "describe",
    "float_from_bits",
    "MAX_WASM_BYTES",
    "client_callback",
    "slider_compute",
    "CallbackSpec",
]

# A pathological kernel must not bloat the component-args wire. M0/M2 kernels are single-KB
# `.wasm`; above this the in-tab path is refused up front and the Python fallback runs.
MAX_WASM_BYTES = 8 * 1024 * 1024

# codex SF-5: the largest int a JS Number carries exactly (Number.MAX_SAFE_INTEGER); the FFI shim's
# `toI64` refuses any Number beyond it, so an int slider range is admitted only inside this bound.
_JS_SAFE_INT = 2**53 - 1

_COMPONENT_DIR = Path(__file__).with_name("wasm_component")

_component = None  # the declared Streamlit component, cached after first WasmComponent() call

# ONE authority for a dispatch's FINAL result (review B5): Streamlit reruns the whole script on
# every interaction, so `result_of`/`deadline_result` are called MANY times for one dispatch. The
# FIRST call that produces a final result (browser value, a browser-error fallback, or a deadline
# timeout) stores it here keyed by nonce; every later call for that (completed) nonce returns the
# STORED result verbatim. Without this, the browser-error branch re-ran the Python body on every
# rerun (`python_calls` drift, SF5) and a deadline result was lost on the next rerun (the app hung
# at "running..." — B5). Bounded so a long session cannot grow it. Each entry stores the FUNCTION
# NAME alongside the result (review SF10) so the cache honours the same per-function scope as
# `_recall` (a wrong-`fn` caller gets the loud unknown, never another kernel's stored result).
_RESULT_HISTORY = 1024
_results: "collections.OrderedDict[int, tuple[str, dict]]" = collections.OrderedDict()
_results_lock = threading.Lock()


def _finalize(nonce: Any, name: str, result: dict) -> dict:
    """Store `result` as the final answer for (`nonce`, `name`) and return it. FIRST-wins (review
    SF11): if a result is already stored for this nonce, keep it and return that one, so two
    finalizations of one dispatch cannot store diverging results."""
    if isinstance(nonce, int) and not isinstance(nonce, bool):
        with _results_lock:
            existing = _results.get(nonce)
            if existing is not None:
                return existing[1]
            _results[nonce] = (name, result)
            while len(_results) > _RESULT_HISTORY:
                _results.popitem(last=False)
    return result


def _final_result(nonce: Any, name: str) -> dict | None:
    """The stored final result for (`nonce`, `name`), or None if not finalized (or stored for a
    DIFFERENT function -- SF10: the cache never crosses the per-function scope)."""
    if not isinstance(nonce, int) or isinstance(nonce, bool):
        return None
    with _results_lock:
        entry = _results.get(nonce)
    return entry[1] if (entry is not None and entry[0] == name) else None


def available() -> bool:
    try:
        import streamlit  # noqa: F401
    except ImportError:
        return False
    return True


def require() -> Any:
    """Import and return the `streamlit` module, or raise with the install hint."""
    try:
        import streamlit
    except ImportError as e:
        raise ImportError(
            "pythscribe.streamlit needs Streamlit: `pip install streamlit` (or `pip install pythscribe[streamlit]`)"
        ) from e
    return streamlit


def WasmComponent() -> Callable[..., Any]:
    """Return the declared Streamlit custom component (lazy: imports Streamlit only here).
    The frontend is VENDORED (`wasm_component/`), so there is no separate install. Call the
    returned function `comp(payload=..., key=...)` to render the iframe; it returns the value
    the iframe published (feed it to `result_of`)."""
    global _component
    if _component is None:
        st = require()
        _component = st.components.v1.declare_component("pythscribe_wasm", path=str(_COMPONENT_DIR))
    return _component


def _wasm_b64(fn: Callable[..., Any]) -> tuple[str | None, int, str | None]:
    """(base64 of the artifact `.wasm`, its size, reason-refused). None when there is no usable
    artifact or the `.wasm` exceeds MAX_WASM_BYTES (browser path refused -> Python fallback)."""
    b = binding_of(fn)
    if b.artifact is None:
        return None, 0, "no usable artifact"
    size = b.artifact.wasm.stat().st_size
    if size > MAX_WASM_BYTES:
        return None, size, f".wasm is {size} B, above the in-args cap of {MAX_WASM_BYTES} B"
    return base64.b64encode(b.artifact.wasm.read_bytes()).decode("ascii"), size, None


def _callback_wasm_b64(spec: "CallbackSpec") -> str:
    """base64 of the callback kernel's `.wasm`, read ONLY from the SERVER-side `CallbackSpec`
    object's `wasm_path` (SF-D, the B-2 trust class). This function accepts a `CallbackSpec` and
    NOTHING else -- never a dict, a `st.session_state` entry, a component return value, or any
    other client-shaped value -- so a `wasm_path` arriving from the client can never cause bytes to
    be read/served. A callback has no server fallback (it must be synchronous), so an oversized /
    missing `.wasm` is refused up front with a clear Python error, never a silent degrade."""
    if not isinstance(spec, CallbackSpec):
        raise TypeError(
            "pythscribe.streamlit: the callback transport is derived ONLY from a server-side "
            f"CallbackSpec (got {type(spec).__name__}); a client-supplied value can never inject "
            "a wasm path (SF-D)"
        )
    if not spec.wasm_path:
        raise RuntimeError(
            f"slider_compute({spec.fn!r}): the CallbackSpec has no wasm_path; build the kernel "
            "(`python -m pythscribe.build <module.py>`)"
        )
    data = Path(spec.wasm_path).read_bytes()
    if len(data) > MAX_WASM_BYTES:
        raise RuntimeError(
            f"slider_compute({spec.fn!r}): .wasm is {len(data)} B, above the in-args cap of "
            f"{MAX_WASM_BYTES} B -- a synchronous client callback has no server fallback (spec §6)"
        )
    return base64.b64encode(data).decode("ascii")


def slider_compute(
    spec: "CallbackSpec",
    *,
    min: float,
    max: float,
    step: float,
    key: str,
    label: str | None = None,
    _test_hold: bool | None = None,
) -> Any:
    """Render an in-iframe slider whose `oninput` calls the compiled `spec` kernel SYNCHRONOUSLY in
    the browser tab and re-renders IN THE IFRAME DOM -- it NEVER calls `setComponentValue` per drag,
    so a slider drag causes ZERO Streamlit server reruns (the class of interaction Streamlit's
    rerun-the-whole-script model cannot do round-trip-free).

    `spec` MUST be a `CallbackSpec` (from `client_callback(fn, shape="node")`) of an arity-1 `node`
    kernel. `key` is REQUIRED (keyword-only, no default -- S-4): with `key`, Streamlit keeps the
    SAME iframe across arg changes (element id = component_name + url + key), so a rebuild / range
    change does not remount the iframe and lose the slider state; without it the base64 `.wasm` is
    part of the hashed identity and any change remounts (and two sliders raise DuplicateWidgetID).

    Returns the component value (the "Export value" payload or `None`); the M0 slider never calls
    `setComponentValue`, so this is `None` on the compute-in-tab path.

    `_test_hold` is a private, test-only hook (S-19 / codex SF-6): `True` is emitted into the
    payload and makes the iframe AWAIT a release barrier (`window.__pythscribe_test_release()`)
    inside its cached instantiate promise -- the window in which the instantiate-once control
    PROVES a same-sha re-render overlapped the in-flight load. Production apps never set it."""
    if not isinstance(spec, CallbackSpec):
        raise TypeError(
            f"slider_compute(spec, ...): spec must be a CallbackSpec from client_callback(...), got "
            f"{type(spec).__name__}"
        )
    if spec.shape != "node":
        raise TypeError(
            f"slider_compute: shape {spec.shape!r} is not slider-hostable in v0.2.5 (the slider hosts "
            "shape='node'); use client_callback(fn, shape='node')"
        )
    if len(spec.param_types) != 1:
        raise TypeError(
            f"slider_compute({spec.fn!r}): the slider drives an arity-1 node kernel, but it has "
            f"{len(spec.param_types)} params {spec.param_types} (a multi-input host is deferred)"
        )
    int_param = spec.param_types == ["int"]
    for nm, v in (("min", min), ("max", max), ("step", step)):
        if isinstance(v, bool) or not isinstance(v, (int, float)):
            raise TypeError(f"slider_compute({spec.fn!r}): {nm} must be an int or float (got {v!r})")
        if not math.isfinite(v):  # a nan/inf bound would silently wedge the <input type=range>
            raise ValueError(f"slider_compute({spec.fn!r}): {nm} must be finite (got {v!r})")
    if int_param:
        # S-6: an int-param kernel needs integral min/max/step (else toI64(2.5) RangeErrors).
        # codex SF-5: AND every position must be a JS SAFE integer -- the frontend `Number`-converts
        # each slider position and the shim's `toI64` refuses |v| > 2**53-1 with a RangeError, so a
        # range outside safe-integer bounds would pass admission and then error on every drag.
        # Refused HERE, up front (the exact bound the shim enforces: _JS_SAFE_INT).
        for nm, v in (("min", min), ("max", max), ("step", step)):
            if abs(int(v)) > _JS_SAFE_INT:  # finite (checked above), so int(v) is exact for a float
                raise ValueError(
                    f"slider_compute({spec.fn!r}): parameter is 'int', so {nm} must be a safe integer "
                    f"(|v| <= 2**53-1) -- the browser slider cannot represent {v!r} exactly"
                )
            if float(v) != int(v):
                raise ValueError(
                    f"slider_compute({spec.fn!r}): parameter is 'int', so {nm} must be integral (got {v!r})"
                )
    if _test_hold is not None and not isinstance(_test_hold, bool):
        raise TypeError("slider_compute: _test_hold must be a bool (test-only hook)")
    # SF-D: wasm_b64 comes ONLY from the server-side spec object; a client value can never reach here.
    wasm_b64 = _callback_wasm_b64(spec)
    payload: dict[str, Any] = {
        "kind": "callback",
        "fn": spec.fn,
        "param_types": list(spec.param_types),
        "return_type": spec.return_type,
        "source_sha256": spec.source_sha256,
        "wasm_b64": wasm_b64,
        "min": int(min) if int_param else float(min),
        "max": int(max) if int_param else float(max),
        "step": int(step) if int_param else float(step),
        "label": label,
    }
    if _test_hold:
        payload["_test_hold"] = True
    comp = WasmComponent()  # the ONE declared component (kind-dispatch); reuses declare_component
    return comp(payload=payload, key=key, default=None)


def dispatch(fn: Callable[..., Any], *args: Any) -> dict[str, Any]:
    """Build the component value for one call. Browser path when a (small enough) artifact is
    bound; otherwise the Python fallback runs HERE and the result is already in the payload.
    Args are checked against the SAME crossable grammar as the Gradio adapter."""
    b: WasmBinding = binding_of(fn)
    b.ensure_compiled()  # compile-on-first-call: no explicit build step needed to get a browser bundle (graceful: never raises)
    # M0 admits exactly ONE crossed return type (float). The one-authority gate (#495, review B3):
    # `_python_result` refuses non-float on the fallback path, so refusing HERE too — at the one
    # boundary BOTH paths share, and the SAME helper the Gradio adapter uses — keeps the contract
    # path-independent (a `-> int`/`-> None` kernel must not silently yield a float / nan on the
    # browser path while the fallback raises TypeError).
    params, ret = _require_scalar_float_return(b)
    _check_crossable(list(args))
    nonce = _next_nonce()
    calls_before, server_before = b.counts()  # one snapshot
    _remember(nonce, b.name, calls_before, tuple(args), server_before)
    wasm_b64, wasm_bytes, refused = _wasm_b64(fn)
    payload: dict[str, Any] = {
        "fn": b.name,
        "args": list(args),
        "param_types": params,
        "return_type": ret,
        "wasm_b64": wasm_b64,
        "wasm_bytes": wasm_bytes,
        "source_sha256": b.source_sha256,
        "artifact_status": b.artifact_status,
        "browser_refused": refused,
        "nonce": nonce,
        "dispatched_at": time.monotonic(),  # server-side clock for the deadline (SF1)
        "result": None,
        "error": None,
    }
    if wasm_b64 is None:  # no artifact / too big -> the Python fallback is produced server-side
        payload["result"] = _python_result(fn, args, calls_before)
        _mark_completed(nonce)
    return payload


def result_of(
    fn: Callable[..., Any], value: dict[str, Any] | None, *, expect_nonce: Any
) -> dict[str, Any] | None:
    """Read a result back from the component value. None while the browser is still computing.
    Only SERVER-side state is trusted for the fallback re-run and the path marker; a browser
    error / malformed result / unknown nonce is answered with the Python fallback on the
    DISPATCHED arguments so the app keeps working.

    `expect_nonce` is REQUIRED (review B4): a KEYED Streamlit component keeps its published value
    across arg changes, so `value` can be a PREVIOUS dispatch's result while a new one is in
    flight. Pass `expect_nonce=payload["nonce"]` (the current dispatch): a value carrying any
    other nonce reads as "still computing" (None) rather than answering the new inputs with the
    old result. There is no default -- omitting it is the exact silent-wrong-value bug this guards.

    A dispatch's FINAL result is computed ONCE and cached by nonce (review B5): every later rerun
    for the same completed nonce returns the STORED result, so the Python body is never re-run on
    a rerun (no `python_calls` drift) and a deadline/error result is never lost."""
    if not value or not isinstance(value, dict):
        return None
    b = binding_of(fn)
    nonce = value.get("nonce")
    if nonce != expect_nonce:
        return None  # a retained value from a PREVIOUS dispatch; the current one is unanswered
    cached = _final_result(nonce, b.name)
    if cached is not None:
        return cached  # idempotent: this dispatch was already finalized (B5 / SF5)
    known = _recall(nonce, b.name)
    if known is None:
        return {
            "value": None, "bits": None, "path": "unknown", "python_calls": None, "server_calls": None,
            "error": "unknown dispatch nonce for this function (stale client state); re-run",
        }
    calls_before, args, server_before = known
    calls_now, server_now = b.counts()  # one snapshot at result time
    server_calls = server_now - server_before
    result = value.get("result")
    if isinstance(result, dict) and result.get("path") == "python-fallback":
        _mark_completed(nonce)
        return _finalize(nonce, b.name, {**result, "server_calls": server_calls})  # produced server-side by dispatch()
    if isinstance(result, dict) and result.get("path") == "browser-wasm":
        v = float_from_bits(result.get("bits"))
        if v is None:  # malformed browser result -> Python fallback, never a silent null
            r = _python_result(fn, args, calls_before)
            r["browser_error"] = "malformed browser result (no valid bits)"
            r["server_calls"] = server_calls
            _mark_completed(nonce)
            return _finalize(nonce, b.name, r)
        r = dict(result)
        r["value"] = v  # reconstructed from bits: -0.0/inf/nan-exact, never the client's JSON number
        r["python_calls"] = calls_now - calls_before  # server-side truth, same snapshot as server_calls
        r["server_calls"] = server_calls
        _mark_completed(nonce)
        return _finalize(nonce, b.name, r)
    if value.get("error"):
        r = _python_result(fn, args, calls_before)
        r["browser_error"] = str(value["error"])
        r["server_calls"] = server_calls
        _mark_completed(nonce)
        return _finalize(nonce, b.name, r)
    return None


def deadline_result(
    fn: Callable[..., Any], payload: dict[str, Any] | None, wait_s: float = 20.0
) -> dict[str, Any] | None:
    """Server-side deadline (SF1; the Streamlit analogue of the Gradio adapter's `deadline_result`,
    non-blocking to suit the rerun model). If the browser has not answered THIS dispatch within
    `wait_s` of its `dispatched_at`, run the Python fallback on the DISPATCHED args and report
    `browser_error='timeout'`. Returns the STORED final result if this dispatch was already
    finalized (so the fallback persists across later reruns -- B5), None while still within the
    deadline or when there is nothing to wait for (no artifact / unknown nonce). Drive re-checks
    from the demo with a short `time.sleep()`+`st.rerun()` poll so a browser that never mounts
    still falls back to Python once the deadline elapses."""
    if not payload or not isinstance(payload, dict) or not payload.get("wasm_b64"):
        return None  # inline fallback already produced (or nothing dispatched)
    b = binding_of(fn)
    nonce = payload.get("nonce")
    cached = _final_result(nonce, b.name)
    if cached is not None:
        return cached  # already finalized (by the browser, an error, or an earlier deadline) -- B5
    started = payload.get("dispatched_at")
    past_deadline = isinstance(started, (int, float)) and (time.monotonic() - started) >= max(0.0, wait_s)
    known = _recall(nonce, b.name)
    if known is None:
        # SF9: the dispatch record was evicted (>_DISPATCH_HISTORY dispatches this process). Past
        # the deadline, return a TERMINAL result so the demo's poll stops (never an endless loop);
        # before it, keep waiting.
        if not past_deadline:
            return None
        return {"value": None, "bits": None, "path": "unknown", "python_calls": None, "server_calls": None,
                "error": "dispatch record evicted before the browser answered; re-run"}
    if is_completed(nonce):
        # completed elsewhere but its result is not in the store any more (evicted -- results
        # evict by finalization order, dispatch records by dispatch order, so a completed nonce
        # can be recalled yet result-evicted). Past the deadline, answer TERMINALLY so the demo
        # poll stops (SF13); before it, keep waiting for the component value.
        if not past_deadline:
            return None
        return {"value": None, "bits": None, "path": "unknown", "python_calls": None, "server_calls": None,
                "error": "result no longer available (evicted after completion); re-run"}
    if not past_deadline:
        return None  # still within the deadline
    calls_before, args, server_before = known
    r = _python_result(fn, args, calls_before)
    r["browser_error"] = f"timeout: no browser result within {wait_s:g} s"
    r["server_calls"] = b.counts()[1] - server_before
    _mark_completed(nonce)
    return _finalize(nonce, b.name, r)


def describe(r: dict[str, Any] | None) -> str:
    if r is None:
        return "running in browser..."
    parts = [
        f"value={r.get('value')!r}",
        f"bits={r.get('bits')}",
        f"path={r.get('path')}",
        f"python_calls={r.get('python_calls')}",
    ]
    if r.get("wasm_how") is not None:
        parts.append(f"wasm_how={r['wasm_how']}")
    if r.get("wasm_export") is not None:
        parts.append(f"wasm_export={r['wasm_export']}")
    if r.get("server_calls") is not None:
        parts.append(f"server_calls={r['server_calls']}")
    if r.get("browser_error"):
        parts.append(f"browser_error={r['browser_error']!r}")
    if r.get("error"):
        parts.append(f"error={r['error']!r}")
    return " ".join(parts)


def __getattr__(name: str) -> Any:
    # Back-compat: the M1.5 stub exposed a `WasmFunction` name that raised NotImplementedError.
    # M3 ships `WasmComponent`; keep a helpful pointer for anyone who used the old name.
    if name == "WasmFunction":
        raise AttributeError("pythscribe.streamlit exposes `WasmComponent` (a declared Streamlit component), not `WasmFunction`")
    raise AttributeError(f"module {__name__!r} has no attribute {name!r}")
