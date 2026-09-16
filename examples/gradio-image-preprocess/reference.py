"""The reference side of the M1 differential: an independent NumPy box downscale, the
packed-pixel helpers, and the checksum both sides (browser JS / server Python) compute over
the resized pixels so a client-side result can be checked against the reference of the
ORIGINAL image without the original ever reaching the server.

Kernel contract (see kernels.py): input = packed 0xRRGGBB ints, row-major; `scale` = an
integer box factor; output pixel = floor(mean over the scale x scale block) per channel;
trailing `w % scale` columns / `h % scale` rows are dropped. `box_reference` implements the
same contract in vectorised NumPy -- a different implementation of the same spec, so the
two agreeing is evidence, not tautology. Pillow's `Image.reduce` (box filter, ROUNDED) is the
second, looser oracle: within +-1 per channel.
"""
from __future__ import annotations

import io
import time
import zlib
from pathlib import Path

import numpy as np
from PIL import Image

JPEG_QUALITY = 85  # the one encoder setting both paths use (client canvas 0.85, server Pillow 85)


def choose_scale(w: int, h: int, max_dim: int) -> int:
    """Plain-Python twin of the `box_scale` kernel contract (used only by tests/notebook to
    cross-check the kernel; the app itself calls the kernel)."""
    big = max(w, h)
    return 1 if big <= max_dim else (big + max_dim - 1) // max_dim


def load_rgb(path: str | Path) -> np.ndarray:
    with Image.open(path) as im:
        return np.asarray(im.convert("RGB"), dtype=np.uint8)


def pack(rgb: np.ndarray) -> np.ndarray:
    """HxWx3 uint8 -> HxW int32 of 0xRRGGBB (the kernel's input format)."""
    a = rgb.astype(np.int32)
    return (a[..., 0] << 16) | (a[..., 1] << 8) | a[..., 2]


def unpack(packed: np.ndarray, w: int, h: int) -> np.ndarray:
    p = np.asarray(packed, dtype=np.int64).reshape(h, w)
    return np.stack([(p >> 16) & 255, (p >> 8) & 255, p & 255], axis=-1).astype(np.uint8)


def box_reference(rgb: np.ndarray, scale: int) -> np.ndarray:
    """Exact integer box downscale (floor of the block sum / area), the kernel's spec."""
    h, w = rgb.shape[:2]
    oh, ow = h // scale, w // scale
    if scale == 1:
        return rgb.copy()
    blk = rgb[: oh * scale, : ow * scale].astype(np.int64).reshape(oh, scale, ow, scale, 3)
    return (blk.sum(axis=(1, 3)) // (scale * scale)).astype(np.uint8)


def pillow_reduce(rgb: np.ndarray, scale: int) -> np.ndarray:
    """Pillow's box reduce on the SAME domain as the kernel: `Image.reduce` averages a
    partial trailing block (5x5 -> 3x3 at scale 2) where the contract drops it, so crop to
    complete blocks first (review r1/S2); the rounding difference (+-1) remains the oracle's."""
    h, w = rgb.shape[:2]
    oh, ow = h // scale, w // scale
    im = Image.fromarray(np.ascontiguousarray(rgb[: oh * scale, : ow * scale]), "RGB")
    return np.asarray(im if scale == 1 else im.reduce(scale), dtype=np.uint8)


def checksum_rgb(rgb: np.ndarray) -> int:
    """CRC-32 over the row-major RGB bytes of a resized image; the JS side computes the same
    over its RGB bytes before the (lossy) canvas encode."""
    return zlib.crc32(np.ascontiguousarray(rgb, dtype=np.uint8).tobytes()) & 0xFFFFFFFF


def encode_jpeg(rgb: np.ndarray, quality: int = JPEG_QUALITY) -> bytes:
    buf = io.BytesIO()
    Image.fromarray(rgb, "RGB").save(buf, format="JPEG", quality=quality)
    return buf.getvalue()


def server_resize(path: str | Path, max_dim: int, quality: int = JPEG_QUALITY) -> dict:
    """The NAIVE control's server-side work, timed: decode the full upload, box-reduce with
    Pillow, encode the small JPEG. Wall + CPU (process) time per phase."""
    t0 = time.perf_counter()
    c0 = time.process_time()
    rgb = load_rgb(path)
    t1 = time.perf_counter()
    h, w = rgb.shape[:2]
    scale = choose_scale(w, h, max_dim)
    small = pillow_reduce(rgb, scale)
    t2 = time.perf_counter()
    jpg = encode_jpeg(small, quality)
    t3 = time.perf_counter()
    c1 = time.process_time()
    return {
        "in_w": w, "in_h": h, "scale": scale, "out_w": small.shape[1], "out_h": small.shape[0],
        "server_decode_ms": (t1 - t0) * 1e3, "server_resize_ms": (t2 - t1) * 1e3,
        "server_encode_ms": (t3 - t2) * 1e3, "server_cpu_ms": (c1 - c0) * 1e3,
        "encoded_bytes": len(jpg), "checksum": checksum_rgb(box_reference(rgb, scale)),
        "checksum_pillow": checksum_rgb(small), "jpeg": jpg,
    }
