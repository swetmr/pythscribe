"""v0.2.5 M1: the image-preprocessing kernels that run IN THE BROWSER before upload.

Both are plain Python (the load-bearing fallback) and are compiled to WASM by the explicit
build step -- never on import:

    python -m pythscribe.build examples/gradio-image-preprocess/kernels.py

`box_scale` picks the integer box factor that brings the long side to <= max_dim.
`downscale_box` is an exact integer box (area-average, floor) downscale of a packed
0xRRGGBB image by that factor, writing the packed result into `out` IN PLACE and
returning only a scalar (the output pixel count): M0's `@wasm` admits scalar returns only,
so the resized pixels stay in WASM/JS memory (plan §4a) and the pythscribe list-buffer FFI
shim reads `out` back from linear memory -- never a big array through the return path.
The kernel's Python semantics (mutating `out`) are exactly what the CPython fallback does.

v0.2.5 M2c adds `downscale_nn`, a FIRST-CLASS TYPED-ARRAY (`Array[uint8, 2]`) nearest-neighbor
downscale (the typed-array path); the box-average `downscale_box` (list-buffer) is unchanged.
`from __future__ import annotations` keeps the bare `Array[uint8, 2]` annotation a lazy string
at runtime (it is not a Python name); the pyths compiler reads it from the source AST.
"""
from __future__ import annotations

from pythscribe import wasm


@wasm
def box_scale(w: int, h: int, max_dim: int) -> int:
    big = w
    if h > big:
        big = h
    if big <= max_dim:
        return 1
    return (big + max_dim - 1) // max_dim


# v0.2.5 M2c: FIRST-CLASS TYPED-ARRAY downscale. `img` is a 2-D uint8 array of shape [H, W*3]
# (row-major RGB bytes -- exactly a NumPy `rgb.reshape(H, W*3)`); `out` is [oh, ow*3], filled IN
# PLACE, returning only the output pixel count (M0 admits scalar returns; the resized bytes stay
# in WASM/typed-array memory and the browser reads `out` back through the pythscribe typed-array
# FFI shim). The transform is NEAREST-NEIGHBOR: output pixel (oy, ox) = input pixel
# (oy*scale, ox*scale). Why nearest-neighbor (not the box-average `downscale_box` below): a box
# average needs integer floor-division (`sum // area`), and `// % << >> & | ^` are NOT yet
# admitted on sub-64-bit (uint8/int32) array kernels -- the fixed-width WRAP would diverge from
# NumPy on an overflowing intermediate -- so a uint8 box-average kernel does not compile to WASM
# today (tracked for M6 / pythscribe #493). Nearest-neighbor uses only `*`/`+` indexing, so it
# admits on uint8. `oh`/`ow` are passed as params (computed by the caller) to keep it `//`-free.
# The box-average demo (`downscale_box`, list-buffer path) is unchanged -- the quality reference;
# this is the typed-array path. NOTE: no docstring -- a string literal is refused on the WASM path.
@wasm
def downscale_nn(img: Array[uint8, 2], scale: int, oh: int, ow: int, out: Array[uint8, 2]) -> int:
    for oy in range(oh):
        iy = oy * scale
        for ox in range(ow):
            ix = ox * scale * 3
            o = ox * 3
            out[oy][o] = img[iy][ix]
            out[oy][o + 1] = img[iy][ix + 1]
            out[oy][o + 2] = img[iy][ix + 2]
    return oh * ow


@wasm
def downscale_box(px: list[int], w: int, h: int, scale: int, out: list[int]) -> int:
    ow = w // scale
    oh = h // scale
    area = scale * scale
    for oy in range(oh):
        for ox in range(ow):
            r = 0
            g = 0
            b = 0
            for dy in range(scale):
                row = (oy * scale + dy) * w
                for dx in range(scale):
                    v = px[row + ox * scale + dx]
                    r = r + ((v >> 16) & 255)
                    g = g + ((v >> 8) & 255)
                    b = b + (v & 255)
            out[oy * ow + ox] = ((r // area) << 16) | ((g // area) << 8) | (b // area)
    return ow * oh
