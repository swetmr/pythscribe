"""M3 E2E -- the runtime-verified "Streamlit running" gate, driven with Playwright.

E2 asserts the BROWSER/WASM path actually ran (RESOLUTION markers, not behaviour parity --
the dual-track-masking lesson):
  * the app's result line says path=browser-wasm and python_calls=0 (server-side counter),
  * wasm_how=bytes (the shim compiled the artifact bytes crossed through the iframe seam),
  * wasm_export=rms_gain (the COMPILED export name, read from the instantiated module -- the
    iframe cannot produce it without actually compiling the artifact .wasm),
  * and the value's IEEE-754 bits equal the Python oracle's.
E3 (the PAIRED NEGATIVE CONTROL) launches a COPY of the demo WITHOUT the artifact directory:
the Python fallback runs (path=python-fallback, python_calls=1, no wasm_export) and the app
still works -- proving the E2 marker DISCRIMINATES the browser path from the fallback and is
not vacuously green.
"""
from __future__ import annotations

import re
import shutil
import struct

import pytest

from _streamlit_harness import DEMO_DIR, StreamlitApp
from conftest import gate_import


@pytest.fixture(autouse=True, scope="module")
def _need_stack():
    gate_import("playwright.sync_api")
    gate_import("streamlit")


@pytest.fixture(scope="module")
def browser():
    from playwright.sync_api import sync_playwright

    with sync_playwright() as p:
        b = p.chromium.launch(headless=True)
        yield b
        b.close()


def _drive(app: StreamlitApp, browser, timeout_ms: int = 90_000):
    ctx = browser.new_context()
    page = ctx.new_page()
    responses: list[tuple[str, int]] = []
    page.on("response", lambda r: responses.append((r.url, r.status)))
    page.goto(app.url, wait_until="load")
    page.get_by_role("button", name="Run rms_gain").wait_for(timeout=timeout_ms)
    page.get_by_role("button", name="Run rms_gain").click()
    page.wait_for_function(
        "() => [...document.querySelectorAll('code')].some(e => /path=/.test(e.textContent))",
        timeout=timeout_ms,
    )
    texts = page.eval_on_selector_all("code", "els => els.map(e => e.textContent)")
    ctx.close()
    line = next(t for t in texts if "path=" in t)
    return line, responses


def parse(text: str) -> dict:
    return dict(re.findall(r"(\w+)=('[^']*'|\S+)", text))


def test_e2_streamlit_runs_kernel_in_browser(browser):
    with StreamlitApp(DEMO_DIR) as app:
        line, responses = _drive(app, browser)
    r = parse(line)
    # E2: the browser/WASM path ran -- resolution markers, not behaviour parity
    assert r["path"] == "browser-wasm", line
    assert r["python_calls"] == "0", line
    assert r["wasm_how"] == "bytes", line
    assert r["wasm_export"] == "rms_gain", line
    assert r["server_calls"] == "0", line
    # the value's bits equal the CPython oracle (D1 for this input): rms_gain([1,2,3,4], 0.5)
    truth = 0.5 / ((1 + 4 + 9 + 16) / 4) ** 0.5
    assert r["bits"] == struct.pack("<d", truth).hex(), (r["bits"], line)
    # the seam is bytes-through-args: NO .wasm was fetched over the wire (the marker proves
    # resolution instead of a fetch count)
    assert not [(u, s) for u, s in responses if u.endswith(".wasm")], "bytes seam must not fetch a .wasm"


def test_b1_two_dispatch_converges_to_the_new_input(browser):
    """B1 CONVERGENCE integration check (NOT the negative control -- review SF6): a KEYED component
    keeps its value across arg changes; a second Run with new inputs must converge to the NEW
    value (nonce 2), exercising the retained-value round-trip end to end. The DETERMINISTIC
    negative control for the nonce-currency fix is the unit test test_u9 (it goes RED if the
    expect_nonce guard is reverted); this E2E does not, and is not claimed to."""
    b1 = struct.pack("<d", 0.5 / ((1 + 4 + 9 + 16) / 4) ** 0.5).hex()  # rms_gain([1,2,3,4], 0.5)
    b2 = struct.pack("<d", 0.5 / ((25 + 36) / 2) ** 0.5).hex()          # rms_gain([5,6], 0.5)
    with StreamlitApp(DEMO_DIR) as app:
        ctx = browser.new_context()
        page = ctx.new_page()
        page.goto(app.url, wait_until="load")
        page.get_by_role("button", name="Run rms_gain").wait_for(timeout=90_000)
        page.get_by_role("button", name="Run rms_gain").click()
        page.wait_for_function(f"() => [...document.querySelectorAll('code')].some(e => e.textContent.includes('{b1}'))", timeout=90_000)
        inp = page.get_by_label("xs (comma-separated floats)")
        inp.fill("5, 6")
        inp.press("Enter")
        page.get_by_role("button", name="Run rms_gain").click()
        # converges to the NEW dispatch's value
        page.wait_for_function(f"() => [...document.querySelectorAll('code')].some(e => e.textContent.includes('{b2}'))", timeout=90_000)
        line = next(t for t in page.eval_on_selector_all("code", "els => els.map(e => e.textContent)") if b2 in t)
        ctx.close()
    r = parse(line)
    assert r["path"] == "browser-wasm" and r["python_calls"] == "0", line


def test_sf2_corrupt_wasm_falls_back_in_the_browser_seam(browser):
    """SF2 in-browser negative control: unlike E3 (which never mounts the iframe), corrupt the
    bundle so the iframe's instantiate THROWS -> it publishes an error -> the app runs the Python
    fallback. Exercises the seam's own failure arm, not the server-side inline fallback."""
    with StreamlitApp(DEMO_DIR, env={"PYTHSCRIBE_M3_CORRUPT_WASM": "1"}) as app:
        line, responses = _drive(app, browser)
    r = parse(line)
    assert r["path"] == "python-fallback", line     # the browser path failed -> Python fallback
    assert r["python_calls"] == "1", line
    assert "browser_error" in line, line             # the failure is surfaced, not swallowed
    truth = 0.5 / ((1 + 4 + 9 + 16) / 4) ** 0.5
    assert r["bits"] == struct.pack("<d", truth).hex(), (r["bits"], line)


def test_b5_never_answering_browser_falls_back_and_persists(browser):
    """B5 / SF1 negative control: if the iframe NEVER answers (here: the shim module is aborted so
    it never mounts), the server-side deadline must fall back to Python AND that result must
    PERSIST across a later rerun (it must not revert to 'running...' forever, the B5 hang). Uses a
    short deadline so the test is fast."""
    truth = struct.pack("<d", 0.5 / ((1 + 4 + 9 + 16) / 4) ** 0.5).hex()
    with StreamlitApp(DEMO_DIR, env={"PYTHSCRIBE_BROWSER_DEADLINE_S": "2"}) as app:
        ctx = browser.new_context()
        page = ctx.new_page()
        page.route("**/list_buffer.mjs", lambda route: route.abort())  # iframe never mounts/answers
        page.goto(app.url, wait_until="load")
        page.get_by_role("button", name="Run rms_gain").wait_for(timeout=90_000)
        page.get_by_role("button", name="Run rms_gain").click()
        # the deadline (2 s) + rerun poll eventually falls back to Python
        page.wait_for_function(
            "() => [...document.querySelectorAll('code')].some(e => /path=python-fallback/.test(e.textContent))",
            timeout=60_000,
        )
        line = next(t for t in page.eval_on_selector_all("code", "els => els.map(e => e.textContent)") if "path=" in t)
        r = parse(line)
        assert r["path"] == "python-fallback" and "timeout" in line, line
        assert r["bits"] == truth, (r["bits"], line)
        # PERSISTENCE (B5): a later rerun (change an input) must STILL show the fallback, not hang
        page.get_by_label("target RMS").fill("0.7")
        page.get_by_label("target RMS").press("Enter")
        page.wait_for_timeout(3000)  # allow reruns to settle
        line2 = next((t for t in page.eval_on_selector_all("code", "els => els.map(e => e.textContent)") if "path=" in t), None)
        assert line2 is not None and "path=python-fallback" in line2, f"fallback did not persist across rerun: {line2!r}"
        ctx.close()


def test_e3_paired_control_fallback_when_no_artifact(browser, tmp_path):
    # a COPY of the demo WITHOUT __pythscribe__/ -> the Python fallback must run
    app_dir = tmp_path / "streamlit-wasm-noart"
    app_dir.mkdir()
    for name in ("app.py", "kernels.py"):
        shutil.copyfile(DEMO_DIR / name, app_dir / name)
    # deliberately do NOT copy __pythscribe__/  (no artifact -> fallback)
    with StreamlitApp(app_dir) as app:
        line, responses = _drive(app, browser)
    r = parse(line)
    assert r["path"] == "python-fallback", line   # the marker DISCRIMINATES: not vacuously browser-wasm
    assert r["python_calls"] == "1", line          # the Python body ran exactly once, server-side
    assert "wasm_export" not in r, line            # no compiled export on the fallback path
    truth = 0.5 / ((1 + 4 + 9 + 16) / 4) ** 0.5
    assert r["bits"] == struct.pack("<d", truth).hex(), (r["bits"], line)  # same value, Python-computed
