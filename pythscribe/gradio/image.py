"""pythscribe.gradio.image -- the image-preprocessing-before-upload adapter (v0.2.5 M1).

The component decodes the user's file IN THE TAB, runs the `@wasm` box downscale through
the list-buffer FFI shim (the kernel's own .wasm, its `out` buffer read back from linear
memory), encodes the small result with the canvas, and uploads THAT as a Gradio file. Only
the small blob crosses to Python; the `@wasm` return channel carries one scalar (the output
pixel count). Plan §4a: the output stays in the browser.

    payload = dispatch_image(downscale_box, box_scale, max_dim=512, quality=0.85)
    comp = WasmFunction(value=payload)
    comp.change(lambda p: handle(image_result_of(downscale_box, p)), [comp], [...])

Path markers (never client-claimed alone): `python_calls` is the PROCESS-GLOBAL delta of the
kernel's Python-body counter since this dispatch (0 on the browser path in a single-session
measurement; another session's fallback can inflate it -- so the app derives its own
per-dispatch marker from whether ITS handler ran the kernel, and keeps this one as a secondary
signal), and the app measures the uploaded bytes on ITS side of the wire. When no usable
artifact exists (or the browser path fails) the component uploads the ORIGINAL and the app
runs the same kernel in Python -- the load-bearing fallback.
"""
from __future__ import annotations

from typing import Any, Callable

from pathlib import Path

from ..decorators import WasmBinding, binding_of
from ..ffi import signature_of
from . import _mark_completed, _next_nonce, _recall, _remember, _static_file_url, is_completed

__all__ = ["dispatch_image", "image_result_of", "wasm_url", "file_path_allowed", "allowed_file_roots"]

IMAGE_KIND = "image"
_PATHS = ("browser-wasm", "upload-original")


def wasm_url(fn: Callable[..., Any]) -> str | None:
    """The served URL of the kernel's .wasm (registers the artifact dir as static); None
    when there is no usable artifact."""
    b = binding_of(fn)
    if b.artifact is None:
        return None
    return _static_file_url(b.artifact.wasm)


def dispatch_image(
    fn: Callable[..., Any],
    scale_fn: Callable[..., Any],
    *,
    max_dim: int = 512,
    quality: float = 0.85,
) -> dict[str, Any]:
    """The component value for ONE upload. `fn` is the buffer kernel
    `(px: list[int], w: int, h: int, scale: int, out: list[int]) -> int`; `scale_fn` the
    scalar `(w: int, h: int, max_dim: int) -> int`. Browser path iff BOTH artifacts resolved."""
    b: WasmBinding = binding_of(fn)
    sb: WasmBinding = binding_of(scale_fn)
    b.ensure_compiled()   # compile-on-first-call: browser bundle needs no explicit build step (graceful: never raises)
    sb.ensure_compiled()
    params, ret = signature_of(b)
    sparams, sret = signature_of(sb)
    if params != ["list[int]", "int", "int", "int", "list[int]"] or ret != "int":
        raise TypeError(f"`{b.name}` must be `(px: list[int], w: int, h: int, scale: int, out: list[int]) -> int`, got {params} -> {ret}")
    if sparams != ["int", "int", "int"] or sret != "int":
        raise TypeError(f"`{sb.name}` must be `(w: int, h: int, max_dim: int) -> int`, got {sparams} -> {sret}")
    if not (isinstance(max_dim, int) and max_dim > 0):
        raise ValueError(f"max_dim must be a positive int, got {max_dim!r}")
    if not (0.0 < float(quality) <= 1.0):
        raise ValueError(f"quality must be in (0, 1], got {quality!r}")
    nonce = _next_nonce()
    calls_before, server_before = b.counts()  # one snapshot (codex m1.5 r2/#2)
    _remember(nonce, b.name, calls_before, (max_dim, float(quality)), server_before)
    wasm = wasm_url(fn)
    swasm = wasm_url(scale_fn)
    browser = wasm is not None and swasm is not None
    return {
        "kind": IMAGE_KIND,
        "fn": b.name,
        "wasm": wasm if browser else None,
        "param_types": params,
        "return_type": ret,
        "scale_fn": sb.name,
        "scale_wasm": swasm if browser else None,
        "scale_param_types": sparams,
        "max_dim": max_dim,
        "quality": float(quality),
        "artifact_status": b.artifact_status if browser else f"{b.artifact_status}/{sb.artifact_status}",
        "source_sha256": b.source_sha256,
        "nonce": nonce,
        "result": None,
        "error": None,
    }


def allowed_file_roots() -> list[Path]:
    """The only directories a client-named file may live in: Gradio's upload/cache folder
    (uploads land there and the component cache is a subtree of it). Gradio's own
    `check_in_upload_folder` is INERT under `mount_gradio_app` (it short-circuits until
    `Blocks.launch()` sets `has_launched` -- opus r1/B1), so this adapter is the authority."""
    from gradio import utils as gu

    return [Path(gu.get_upload_folder()).resolve()]


def _is_under(path: Path, root: Path) -> bool:
    try:
        path.relative_to(root)
        return True
    except ValueError:
        return False


def file_path_allowed(path: str) -> bool:
    """True iff `path` (a client-named server path) resolves to a regular file inside an
    allowed root -- symlinks resolved first, so a link out of the folder is refused too."""
    try:
        p = Path(path).resolve(strict=True)
    except (OSError, RuntimeError, ValueError):
        return False
    return p.is_file() and any(_is_under(p, root) for root in allowed_file_roots())


def _file_of(result: dict[str, Any]) -> dict[str, Any] | None:
    f = result.get("file")
    if isinstance(f, dict) and isinstance(f.get("path"), str) and f["path"] and file_path_allowed(f["path"]):
        return f
    return None


def image_result_of(fn: Callable[..., Any], payload: dict[str, Any] | None) -> dict[str, Any] | None:
    """Read the component's result back. None while nothing has been uploaded for this
    dispatch. Otherwise a dict with the client's fields PLUS the server-side truth:
    `python_calls` (process-global kernel Python-body runs since dispatch -- 0 on the
    browser path of a single session; see the module docstring),
    `calls_before` (so the app can re-derive the delta after running the fallback itself),
    `max_dim`/`quality` as DISPATCHED (never the client's copy). A malformed result, an
    unknown nonce, or a client-reported error is answered with path='upload-original'
    when a file is present (the app runs the Python fallback) or an error otherwise."""
    if not payload or not isinstance(payload, dict) or payload.get("kind") != IMAGE_KIND:
        return None
    b = binding_of(fn)
    nonce = payload.get("nonce")
    known = _recall(nonce, b.name)
    if known is None:
        return {"path": "unknown", "python_calls": None, "server_calls": None, "calls_before": None, "file": None,
                "error": "unknown dispatch nonce for this function (stale client state); reload"}
    calls_before, (max_dim, quality), server_before = known
    result = payload.get("result")
    err = payload.get("error")
    if not isinstance(result, dict) and not err:
        return None  # nothing uploaded yet (a resultless echo of an answered nonce is simply idle -- opus r2/NS-8)
    if is_completed(nonce):  # opus r1/S7: a dispatch answers exactly once; a replayed RESULT is refused
        return {"path": "replay", "python_calls": None, "server_calls": None, "calls_before": None, "file": None,
                "error": "this dispatch nonce was already answered; a result cannot be replayed"}
    out: dict[str, Any] = dict(result) if isinstance(result, dict) else {}
    file = _file_of(out)
    path = out.get("path")
    if file is None:
        _mark_completed(nonce)
        raw = out.get("file")
        why = "no file in result" if not (isinstance(raw, dict) and raw.get("path")) else "file path is outside Gradio's upload folder (refused)"
        calls_now, server_now = b.counts()
        return {**out, "path": "error", "file": None, "python_calls": calls_now - calls_before, "server_calls": server_now - server_before,
                "calls_before": calls_before, "max_dim": max_dim, "quality": quality,
                "error": str(err or out.get("browser_error") or why)}
    if path not in _PATHS or err:
        # a browser-side failure that still uploaded the original -> the app runs the fallback
        out["browser_error"] = str(err or out.get("browser_error") or f"unexpected path {path!r}")
        path = "upload-original"
    out["path"] = path
    out["file"] = file
    calls_now, server_now = b.counts()  # one snapshot at result time
    out["python_calls"] = calls_now - calls_before
    out["server_calls"] = server_now - server_before  # M1.5: the in-process WASM marker, same snapshot as python_calls
    out["calls_before"] = calls_before
    out["max_dim"] = max_dim
    out["quality"] = quality
    out["nonce"] = nonce
    _mark_completed(nonce)
    return out


def image_dispatch_completed(nonce: Any) -> bool:
    return is_completed(nonce)
