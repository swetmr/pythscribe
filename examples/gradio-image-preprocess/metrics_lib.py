"""The measurement code behind metrics.ipynb -- a module so the notebook stays short and
the derivation can be TESTED (a hardcoded table would not change with the inputs; the tests
assert the table is a function of the records, and the records of the images).

Everything measured here comes from real runs:
  * `collect()` launches the demo app and drives BOTH panels with a real headless browser,
    per image; the bytes come from the server-side meter (`upload_bytes`), never from the client.
  * `cpython_kernel_ms()` times the SAME kernel in CPython in-process (apples-to-apples
    'server CPU if this resize ran server-side in Python'); the naive panel's Pillow timings
    are the realistic server baseline.
"""
from __future__ import annotations

import hashlib
import json
import sys
import time
from pathlib import Path

import numpy as np

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import reference as R  # noqa: E402

TEST_IMAGES_DIR = HERE / "test_images"


def committed_images() -> list[Path]:
    """The committed SPOT set, verified against its manifest (a drifted set is refused)."""
    manifest = json.loads((TEST_IMAGES_DIR / "manifest.json").read_text(encoding="utf-8"))
    out = []
    for name, meta in sorted(manifest.items(), key=lambda kv: kv[1]["w"] * kv[1]["h"]):
        p = TEST_IMAGES_DIR / name
        data = p.read_bytes()
        if hashlib.sha256(data).hexdigest() != meta["sha256"] or len(data) != meta["bytes"]:
            raise RuntimeError(f"{name} does not match test_images/manifest.json; run make_test_images.py")
        out.append(p)
    return out


def collect(images: list[Path], *, app_dir: Path | None = None, env: dict | None = None, repeats: int = 1) -> list[dict]:
    """Drive client + naive for every image, `repeats` times; returns the server records
    (each tagged with `image` and `rep`). Runs `drive.py --collect` in a SUBPROCESS: Playwright's
    sync API refuses a thread with a live asyncio loop (a Jupyter kernel has one), and the
    kernel's Windows selector loop cannot spawn Playwright's driver -- a fresh interpreter has
    neither problem, in a script or in a notebook."""
    import os
    import subprocess
    import sys
    import tempfile

    d = Path(tempfile.mkdtemp(prefix="m1collect_"))
    spec = d / "spec.json"
    out = d / "records.json"
    spec.write_text(json.dumps({"images": [str(Path(p).resolve()) for p in images], "app_dir": str(app_dir) if app_dir else None, "env": env, "repeats": repeats}), encoding="utf-8")
    proc = subprocess.run([sys.executable, str(HERE / "drive.py"), "--collect", str(spec), str(out)],
                          cwd=str(HERE), env={**os.environ, "PYTHONUTF8": "1"}, capture_output=True, text=True, check=False)
    if proc.returncode != 0 or not out.is_file():
        raise RuntimeError(f"collection subprocess failed (exit {proc.returncode}):\n{proc.stderr[-4000:]}")
    return json.loads(out.read_text(encoding="utf-8"))


def cpython_kernel_ms(images: list[Path], max_dim: int = 512) -> dict[str, dict]:
    """Time the plain-Python kernel (the @wasm fallback) on each image, in-process.

    The PYTHON BODY, explicitly (opus m1.5 r1/B2): since M1.5 a bare call of an artifact-bound
    kernel runs the in-process WASM when wasmtime is installed, which would publish wasmtime
    timings under the CPython label. The marker assertion binds the label to the path."""
    from kernels import box_scale, downscale_box

    from pythscribe import binding_of

    scale_b, down_b = binding_of(box_scale), binding_of(downscale_box)
    out = {}
    for img in images:
        rgb = R.load_rgb(img)
        h, w = rgb.shape[:2]
        scale = scale_b.run_python(w, h, max_dim)
        ow, oh = w // scale, h // scale
        px = R.pack(rgb).ravel().tolist()
        buf = [0] * (ow * oh)
        before = down_b.calls()
        t0 = time.perf_counter()
        c0 = time.process_time()
        n = down_b.run_python(px, w, h, scale, buf)
        wall = (time.perf_counter() - t0) * 1e3
        cpu = (time.process_time() - c0) * 1e3
        assert down_b.calls() == before + 1, "the timed region did not run the Python body"
        assert n == ow * oh
        exact = bool(np.array_equal(R.unpack(np.asarray(buf), ow, oh), R.box_reference(rgb, scale)))
        out[img.name] = {"cpython_kernel_ms": wall, "cpython_kernel_cpu_ms": cpu, "exact_vs_reference": exact, "scale": scale}
    return out


BROWSER_PATH = "browser-wasm"


def summarize(records: list[dict], cpython: dict[str, dict] | None = None, *, allow_fallback: bool = False) -> dict:
    """Per-image rows + aggregate, from the records ONLY (medians over repeats).

    Server-derived fields are the authority: input dimensions/scale come from the NAIVE
    record (the server decoded the original there), output dimensions from the client
    record's server-decoded received file, bytes from the meter. Browser-reported values
    (under `client`) are only ever COMPARED against server values (`checksum_match`) or
    reported as client timings. Unless `allow_fallback=True`, a client record whose server-
    decided path is not 'browser-wasm' is REFUSED (review r1/B4): the '@wasm in the tab'
    arm must have run in the tab, or the table must say so explicitly."""
    by: dict[str, dict[str, list[dict]]] = {}
    for r in records:
        by.setdefault(r["image"], {}).setdefault(r["mode"], []).append(r)

    def med(rs: list[dict], key: str) -> float:
        vals = [float(r[key]) for r in rs if r.get(key) is not None]
        return float(np.median(vals)) if vals else float("nan")

    def med_client_ms(rs: list[dict], key: str) -> float:
        vals = [float(r["client"]["ms"][key]) for r in rs if r.get("client", {}).get("ms", {}).get(key) is not None]
        return float(np.median(vals)) if vals else float("nan")

    rows = []
    for image, modes in by.items():
        c, n = modes.get("client", []), modes.get("naive", [])
        if not c or not n:
            raise ValueError(f"{image}: need both client and naive records, got {sorted(modes)}")
        if any(not r.get("upload_measured") for r in c + n):
            raise ValueError(f"{image}: an upload was not measured by the server meter")
        paths = sorted({r["path"] for r in c})
        if not allow_fallback and paths != [BROWSER_PATH]:
            raise ValueError(f"{image}: client records did not all run in the tab (server-decided path {paths}); pass allow_fallback=True to tabulate a fallback run AS a fallback")
        if any(r.get("python_calls") != 0 for r in c) and not allow_fallback:
            raise ValueError(f"{image}: the server ran the kernel's Python body for a client record")
        if any(r.get("consistent") is False for r in c):
            raise ValueError(f"{image}: a client record's story is inconsistent with what the server received")
        n0 = n[0]
        client_bytes = med(c, "upload_bytes")
        naive_bytes = med(n, "upload_bytes")
        # wire (upload body + event body) only when EVERY record measured both; never understated
        wire_ok = all(r.get("wire_measured") for r in c + n)
        client_wire = med(c, "wire_bytes") if wire_ok else float("nan")
        naive_wire = med(n, "wire_bytes") if wire_ok else float("nan")
        if c[0]["path"] == BROWSER_PATH:
            checksum_match = all(r["client"].get("checksum") == n0.get("checksum") for r in c)
        else:
            checksum_match = all(r.get("checksum") == n0.get("checksum") for r in c)
        row = {
            "image": image,
            "pixels": int(n0["in_w"]) * int(n0["in_h"]),
            "in": f"{n0['in_w']}x{n0['in_h']}",
            "out": f"{c[0]['out_w']}x{c[0]['out_h']}",
            "scale": int(n0["scale"]),
            "client_path": "+".join(paths),
            "naive_upload_bytes": naive_bytes,
            "client_upload_bytes": client_bytes,
            "reduction_x": naive_bytes / client_bytes,
            "reduction_pct": 100.0 * (1.0 - client_bytes / naive_bytes),
            "naive_join_bytes": med(n, "join_bytes"),
            "client_join_bytes": med(c, "join_bytes"),
            "naive_wire_bytes": naive_wire,
            "client_wire_bytes": client_wire,
            "wire_measured": wire_ok,
            "wire_reduction_x": (naive_wire / client_wire) if wire_ok else float("nan"),
            "naive_server_resize_ms": med(n, "server_resize_ms"),
            "naive_server_decode_ms": med(n, "server_decode_ms"),
            "naive_server_cpu_ms": med(n, "server_cpu_ms"),
            "client_server_cpu_ms": med(c, "server_cpu_ms"),
            "client_wasm_call_ms": med_client_ms(c, "wasm_call_ms"),
            "client_total_ms": med_client_ms(c, "total_ms"),
            "checksum_match": checksum_match,
        }
        if cpython and image in cpython:
            row["cpython_kernel_ms"] = cpython[image]["cpython_kernel_ms"]
        rows.append(row)
    rows.sort(key=lambda r: r["pixels"])
    tot_n = sum(r["naive_upload_bytes"] for r in rows)
    tot_c = sum(r["client_upload_bytes"] for r in rows)
    wire_all = all(r["wire_measured"] for r in rows)
    tot_nw = sum(r["naive_wire_bytes"] for r in rows) if wire_all else float("nan")
    tot_cw = sum(r["client_wire_bytes"] for r in rows) if wire_all else float("nan")
    agg = {
        "images": len(rows),
        "client_paths": "+".join(sorted({p for r in rows for p in r["client_path"].split("+")})),
        "naive_upload_bytes_total": tot_n,
        "client_upload_bytes_total": tot_c,
        "reduction_x": tot_n / tot_c,
        "reduction_pct": 100.0 * (1.0 - tot_c / tot_n),
        "naive_wire_bytes_total": tot_nw,
        "client_wire_bytes_total": tot_cw,
        "wire_measured": wire_all,
        "wire_reduction_x": (tot_nw / tot_cw) if wire_all else float("nan"),
        "server_resize_ms_saved_total": sum(r["naive_server_resize_ms"] for r in rows),
        "server_cpu_ms_naive_total": sum(r["naive_server_cpu_ms"] for r in rows),
        "server_cpu_ms_client_total": sum(r["client_server_cpu_ms"] for r in rows),
    }
    if cpython:
        agg["cpython_kernel_ms_total"] = sum(r.get("cpython_kernel_ms", 0.0) for r in rows)
    return {"rows": rows, "aggregate": agg}


def format_table(summary: dict) -> str:
    rows = summary["rows"]
    has_cp = any("cpython_kernel_ms" in r for r in rows)
    head = ("| image | in -> out (scale) | client path (server-decided) | naive upload B | client upload B | reduction | "
            "naive wire B (upload+event) | client wire B | wire reduction | server resize ms (Pillow, naive) | server CPU ms naive -> client | WASM call ms (tab)")
    head += " | same kernel in CPython ms |" if has_cp else " |"
    sep = "|" + "---|" * (head.count("|") - 1)
    lines = [head, sep]
    for r in rows:
        line = (f"| {r['image']} | {r['in']} -> {r['out']} ({r['scale']}) | {r['client_path']} | {r['naive_upload_bytes']:,.0f} | {r['client_upload_bytes']:,.0f} "
                f"| {r['reduction_x']:.1f}x ({r['reduction_pct']:.1f}%) | {r['naive_wire_bytes']:,.0f} | {r['client_wire_bytes']:,.0f} | {r['wire_reduction_x']:.1f}x "
                f"| {r['naive_server_resize_ms']:.1f} | {r['naive_server_cpu_ms']:.1f} -> {r['client_server_cpu_ms']:.1f} | {r['client_wasm_call_ms']:.1f}")
        line += f" | {r['cpython_kernel_ms']:.0f} |" if has_cp else " |"
        lines.append(line)
    a = summary["aggregate"]
    lines.append(f"| **all {a['images']}** | | {a['client_paths']} | {a['naive_upload_bytes_total']:,.0f} | {a['client_upload_bytes_total']:,.0f} | **{a['reduction_x']:.1f}x ({a['reduction_pct']:.1f}%)** "
                 f"| {a['naive_wire_bytes_total']:,.0f} | {a['client_wire_bytes_total']:,.0f} | **{a['wire_reduction_x']:.1f}x** | {a['server_resize_ms_saved_total']:.1f} | {a['server_cpu_ms_naive_total']:.1f} -> {a['server_cpu_ms_client_total']:.1f} | |"
                 + (f" {a.get('cpython_kernel_ms_total', 0):.0f} |" if has_cp else ""))
    return "\n".join(lines)


def plot(summary: dict, path: Path) -> Path:
    import matplotlib

    matplotlib.use("Agg")
    import matplotlib.pyplot as plt

    rows = summary["rows"]
    x = np.arange(len(rows))
    fig, ax = plt.subplots(figsize=(7, 3.6))
    paths = summary["aggregate"]["client_paths"]
    client_label = "client: @wasm resize in the tab, then upload" if paths == BROWSER_PATH else f"client panel (server-decided path: {paths})"
    ax.bar(x - 0.2, [r["naive_upload_bytes"] / 1e3 for r in rows], 0.4, label="naive: upload the original")
    ax.bar(x + 0.2, [r["client_upload_bytes"] / 1e3 for r in rows], 0.4, label=client_label)
    ax.set_xticks(x, [f"{r['in']}\n{r['reduction_x']:.0f}x smaller" for r in rows])
    ax.set_ylabel("bytes uploaded (KB, server-measured)")
    ax.set_yscale("log")
    ax.set_title(f"bytes uploaded per image -- aggregate reduction {summary['aggregate']['reduction_x']:.1f}x ({summary['aggregate']['reduction_pct']:.1f}%)")
    ax.legend(fontsize=8)
    fig.tight_layout()
    fig.savefig(path, dpi=120)
    plt.close(fig)
    return path


def throughput_recipe(summary: dict) -> str:
    """Plan §9 asks for requests-served-per-hour on a fixed tier. Not measured here (it
    needs a fixed public tier + a load generator); the first-order model and the recipe."""
    a = summary["aggregate"]
    rows = summary["rows"]
    big = rows[-1]
    return (
        "Throughput (requests/hour on a fixed tier) is NOT measured in this notebook -- no fabricated number.\n"
        "First-order model: per request the server spends  t = bytes / link_rate + server_cpu.\n"
        f"  naive  (largest image): {big['naive_upload_bytes']:,.0f} B + {big['naive_server_cpu_ms']:.0f} ms CPU\n"
        f"  client (largest image): {big['client_upload_bytes']:,.0f} B + {big['client_server_cpu_ms']:.0f} ms CPU\n"
        "  => on a link where the upload dominates, requests/hour scales with the bytes ratio "
        f"({big['reduction_x']:.0f}x for that image; {a['reduction_x']:.1f}x aggregate).\n"
        "Recipe (run it against a deployed Space, both panels, same tier):\n"
        "  k6 run --vus 20 --duration 10m upload.js   # POST the original to /gradio_api/upload + /gradio_api/queue/join (naive)\n"
        "  k6 run --vus 20 --duration 10m upload.js -e MODE=client   # POST the pre-resized JPEG (what the tab would send)\n"
        "  compare http_reqs/s and p95 latency; the CPU side is visible in the Space's metrics."
    )
