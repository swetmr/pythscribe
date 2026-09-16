"""Regenerate the committed SPOT image set deterministically (seeded synthetic "photos").

    python make_test_images.py            # writes test_images/*.jpg + test_images/manifest.json

Synthetic on purpose: no 12 MB photo in git, no licensing, and anyone can regenerate the
exact bytes (Pillow's JPEG encoder is deterministic for a given libjpeg build; the manifest
pins sha256 + dimensions so a drifted set is detected, not silently measured).
"""
from __future__ import annotations

import hashlib
import json
import sys
from pathlib import Path

import numpy as np
from PIL import Image

HERE = Path(__file__).resolve().parent
OUT = HERE / "test_images"
SIZES = [(640, 480), (1600, 1200), (4000, 3000)]  # 0.3 / 1.9 / 12 MP (~3 MB committed in total)
SEED = 20260902
JPEG_QUALITY_SOURCE = 90  # a camera-like source quality (the ORIGINAL the naive path uploads)


def synth(w: int, h: int, seed: int) -> np.ndarray:
    rng = np.random.default_rng(seed)
    y, x = np.mgrid[0:h, 0:w].astype(np.float32)
    u, v = x / w, y / h
    # sky gradient + sun + rolling hills + textured ground + a little sensor noise
    sky_r = 90 + 120 * v
    sky_g = 140 + 80 * v
    sky_b = 235 - 60 * v
    sun = np.exp(-(((u - 0.72) ** 2 + (v - 0.28) ** 2) * 60.0)) * 255
    hills = 0.55 + 0.08 * np.sin(u * 9.0) + 0.05 * np.sin(u * 23.0 + 1.3) + 0.03 * np.cos(u * 41.0)
    ground = v > hills
    tex = rng.random((h, w), dtype=np.float32) * 38.0
    r = np.where(ground, 60 + 40 * (v - hills) * 4 + tex * 0.9, sky_r + sun)
    g = np.where(ground, 110 + 60 * (v - hills) * 4 + tex, sky_g + sun * 0.9)
    b = np.where(ground, 40 + 20 * (v - hills) * 4 + tex * 0.5, sky_b + sun * 0.6)
    noise = rng.normal(0.0, 3.0, (h, w, 3)).astype(np.float32)
    rgb = np.stack([r, g, b], axis=-1) + noise
    return np.clip(rgb, 0, 255).astype(np.uint8)


def main() -> int:
    OUT.mkdir(exist_ok=True)
    manifest = {}
    for i, (w, h) in enumerate(SIZES):
        name = f"photo_{w}x{h}.jpg"
        p = OUT / name
        Image.fromarray(synth(w, h, SEED + i), "RGB").save(p, format="JPEG", quality=JPEG_QUALITY_SOURCE, optimize=True)
        data = p.read_bytes()
        manifest[name] = {"w": w, "h": h, "bytes": len(data), "sha256": hashlib.sha256(data).hexdigest()}
        print(f"{name}: {len(data):,} B")
    (OUT / "manifest.json").write_text(json.dumps(manifest, indent=2, sort_keys=True) + "\n", encoding="utf-8", newline="\n")
    return 0


if __name__ == "__main__":
    sys.exit(main())
