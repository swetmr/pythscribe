"""E1 / E2 / E3 -- the runtime-verified "gradio running" gate, driven with Playwright.

E2 asserts the BROWSER path actually ran (resolution markers), not just the number:
  * the app's result line says path=browser-wasm and python_calls=0 (server-side counter),
  * the tab issued a request for the .wasm that returned 200,
  * the component recorded a .wasm resource-timing entry (wasm_fetched=1),
  * and the value's IEEE-754 bits equal the Python oracle's (D1 for that input).
E3 launches a COPY of the demo without the artifact directory: the Python fallback runs
(path=python-fallback, python_calls=1, no .wasm request) and the app still works.
"""
from __future__ import annotations

import re
import shutil

import pytest

from _app_harness import DEMO_DIR, GradioApp
from pythscribe.build.runner import float_bits
from test_differential import python_oracle  # the CPython arm, explicitly (a bare call runs the WASM under mode server -- codex m1.5 r2/#5)

from conftest import gate_import


@pytest.fixture(autouse=True, scope="module")
def _need_browser_stack():
    gate_import("playwright.sync_api")
    gate_import("gradio_wasmfunction")


playwright = None

XS = "1, 2, 3, 4"
TARGET = 0.5


@pytest.fixture(scope="module")
def browser():
    from playwright.sync_api import sync_playwright

    with sync_playwright() as p:
        b = p.chromium.launch(headless=True)
        yield b
        b.close()


def drive(app: GradioApp, browser, xs: str = XS, target: float = TARGET, timeout_ms: int = 90_000):
    ctx = browser.new_context()
    page = ctx.new_page()
    responses: list[tuple[str, int]] = []
    console_errors: list[str] = []
    page.on("response", lambda r: responses.append((r.url, r.status)))
    page.on("console", lambda m: console_errors.append(m.text) if m.type == "error" else None)
    page.on("pageerror", lambda e: console_errors.append(f"pageerror: {e}"))
    page.goto(app.url, wait_until="load")
    page.wait_for_selector("#run", timeout=timeout_ms)
    page.wait_for_selector("[data-testid=wasm-status]", timeout=timeout_ms)
    mount_errors = list(console_errors)
    page.locator("#xs textarea").fill(xs)
    page.locator("#target input").fill(str(target))
    page.locator("#run").click()
    page.wait_for_function(
        "() => { const t = document.querySelector('#result textarea'); return !!t && /path=/.test(t.value); }",
        timeout=timeout_ms,
    )
    text = page.locator("#result textarea").input_value()
    status = page.locator("[data-testid=wasm-status]").inner_text()
    ctx.close()
    return text, status, responses, mount_errors, console_errors


def parse(text: str) -> dict:
    # values are repr()s (quoted when they contain spaces) or bare tokens
    return dict(re.findall(r"(\w+)=('[^']*'|\S+)", text))


def test_e1_e2_app_launches_and_runs_kernel_in_browser(browser, demo_artifact, demo_kernels):
    with GradioApp(DEMO_DIR) as app:
        text, status, responses, mount_errors, errors = drive(app, browser)
    r = parse(text)
    # E1: launched, component mounted, no console error on mount
    assert mount_errors == [], mount_errors
    # E2: the browser/WASM path ran -- resolution markers, not behaviour parity
    assert r["path"] == "browser-wasm", text
    assert r["python_calls"] == "0", text
    assert r["wasm_fetched"] == "1", text
    wasm_hits = [(u, s) for u, s in responses if u.endswith("rms_gain.wasm")]
    assert wasm_hits and all(s == 200 for _, s in wasm_hits), (wasm_hits, responses[-10:])
    js_hits = [(u, s) for u, s in responses if u.endswith("rms_gain.js") or u.endswith("rms_gain.glue.js")]
    assert len(js_hits) >= 2 and all(s == 200 for _, s in js_hits), js_hits
    assert status.startswith("done (browser-wasm)"), status
    # and the value is the D1 oracle for this input, bit-for-bit
    oracle = python_oracle(demo_kernels, [1.0, 2.0, 3.0, 4.0], 0.5)
    assert r["bits"] == float_bits(oracle), (r, oracle)
    assert float(r["value"]) == oracle
    assert errors == [], errors


def test_e3_fallback_fires_when_artifact_absent(browser, tmp_path, demo_kernels):
    app_dir = tmp_path / "gradio-wasm-noartifact"
    app_dir.mkdir()
    for f in ("app.py", "kernels.py"):
        shutil.copyfile(DEMO_DIR / f, app_dir / f)
    assert not (app_dir / "__pythscribe__").exists()
    with GradioApp(app_dir) as app:
        text, status, responses, mount_errors, errors = drive(app, browser)
    r = parse(text)
    assert mount_errors == [], mount_errors
    assert r["path"] == "python-fallback", text
    assert r["python_calls"] == "1", text
    assert not any(u.endswith(".wasm") for u, _ in responses), "fallback run must not fetch a .wasm"
    oracle = python_oracle(demo_kernels, [1.0, 2.0, 3.0, 4.0], 0.5)
    assert r["bits"] == float_bits(oracle)
    assert errors == [], errors


def test_e2_browser_resolution_marker_wasm_not_js_twin(browser, tmp_path, demo_artifact, demo_kernels):
    """Serve a RE-SIGNED mutant artifact whose JS twin is poisoned (manifest hashes recomputed so
    the decorator binds it). The browser result must still equal the oracle: the .wasm in the
    tab computed it, not the glue's JS fallback twin."""
    import json

    from pythscribe.artifacts import MANIFEST_NAME, manifest_self_hash, sha256_file, verify
    from test_differential import poison_js_twin

    app_dir = tmp_path / "gradio-wasm-twinpoison"
    app_dir.mkdir()
    for f in ("app.py", "kernels.py"):
        shutil.copyfile(DEMO_DIR / f, app_dir / f)
    adir = app_dir / "__pythscribe__" / "rms_gain"
    shutil.copytree(demo_artifact.dir, adir)
    poison_js_twin(adir)
    m = json.loads((adir / MANIFEST_NAME).read_text(encoding="utf-8"))
    m["files"]["rms_gain.glue.js"] = sha256_file(adir / "rms_gain.glue.js")
    m["manifest_sha256"] = manifest_self_hash(m)  # re-sign (a hand edit without this is refused)
    (adir / MANIFEST_NAME).write_text(json.dumps(m, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    verify(adir, function="rms_gain", expected_source_sha256=demo_artifact.source_sha256)

    with GradioApp(app_dir) as app:
        text, status, responses, mount_errors, errors = drive(app, browser)
    r = parse(text)
    assert r["path"] == "browser-wasm" and r["python_calls"] == "0", text
    assert any(u.endswith("rms_gain.wasm") and s == 200 for u, s in responses)
    oracle = python_oracle(demo_kernels, [1.0, 2.0, 3.0, 4.0], 0.5)
    assert r["bits"] == float_bits(oracle), text
    assert float(r["value"]) != 12345.0
