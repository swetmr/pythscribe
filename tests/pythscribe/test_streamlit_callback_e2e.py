"""M0 go/no-go E2E (v0.2.5 Streamlit callback path) -- driven with Playwright against a REAL
`streamlit run` app (`examples/streamlit-callback/`).

The runtime-verified exit criterion (written before the code):

  An in-iframe `<input type=range>` whose handler calls a once-instantiated `@wasm` kernel
  SYNCHRONOUSLY and re-renders IN THE IFRAME DOM without `setComponentValue`, so the Streamlit
  server script NEVER reruns on a drag -- headless-verified: server-rerun-count delta 0 across a
  drag, `rerun_script` WS-frame delta 0, and the value == the CPython oracle bit-exact; a paired
  NATIVE `st.slider` -> server-recompute control DOES trip the counters.

Anti-vacuity: the marker triple (path=browser-wasm AND server-rerun delta 0 AND rerun_script frame
delta 0) is non-vacuous ONLY because the paired native-`st.slider` control trips BOTH counters
(test_m0_native_control_trips_counter). The value assertion is non-vacuous because, per drag, the
CONSUMED input (`data-x-bits`) must equal the slider's ACTUAL DOM value (which must have moved to the
requested x) BEFORE the result is compared to the oracle of that value (codex B1: without the
consumed==actual check, a handler that never recomputes leaves every drag at the initial x and
`value == oracle(x)` still passes). The instantiate-once control uses a RELEASE BARRIER so the
same-sha re-render PROVABLY overlaps the in-flight instantiate (codex SF-6).

E2E MUTANTS (every control below is paired): a MUTATED copy of the vendored component is served
through a test-authored witness app that points `pythscribe.streamlit._COMPONENT_DIR` at the copy
(a test-side override; no production hook) -- so each RED is observed against the real transport,
not a model of it.

KERNEL OWNERSHIP: the witness app compiles its OWN test-authored kernels (`_WITNESS_KERNELS`,
built per test with `pythscribe.build.build_module`), so the suite is decoupled from the demo's
`kernels.py`, which is a product artifact that evolves (2026-09-16: it became a single 10M-iteration
kernel and dropped `response_alt`, which silently broke every witness flow). The tests that drive the
REAL demo app (`examples/streamlit-callback/`) compare against the demo's own plain-Python twin
`response_py` -- imported from the demo file, never mirrored by hand, so the oracle cannot drift.
"""
from __future__ import annotations

import os
import shutil
import struct
from pathlib import Path

import pytest

from _streamlit_harness import REPO, StreamlitApp
from conftest import gate_import, import_module_from

CALLBACK_DIR = REPO / "examples" / "streamlit-callback"
COMPONENT_DIR = REPO / "pythscribe" / "streamlit" / "wasm_component"


def oracle(x: float) -> float:
    """The CPython reference for the WITNESS app's `response` -- exact float arithmetic, mirroring
    `_WITNESS_KERNELS::response` (test-owned; both live in this file). Computed per-x (never a
    hand-typed constant); fed the SAME x the kernel consumed (decoded from `data-x-bits`, S-5)."""
    return (x * x) * 0.1 - x + 3.0


def oracle_alt(x: float) -> float:
    """CPython reference for the witness `response_alt` (differs from `response` at every x)."""
    return (x * x) * 0.1 - x + 3.0 + 1.0


@pytest.fixture(scope="module")
def demo_oracle():
    """The CPython reference for the REAL demo's `@wasm` kernel: the demo's OWN plain-Python twin
    `response_py` ("SAME math as `response`"), imported from `examples/streamlit-callback/kernels.py`
    itself (unique module name; never a hand-typed mirror, so a demo edit cannot leave a stale oracle
    behind). ~1 s per call in CPython (10M iterations) -- used only by the real-demo drag sequence."""
    m = import_module_from(CALLBACK_DIR / "kernels.py", "kernels_streamlit_callback_for_tests")
    return m.response_py


def bits_of(x: float) -> str:
    return struct.pack("<d", x).hex()


def unbits(h: str) -> float:
    return struct.unpack("<d", bytes.fromhex(h))[0]


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


# ---- helpers --------------------------------------------------------------------------------

def _cb_frame(page):
    """The component iframe (the one holding the callback UI). Located by content (#cb-result),
    robust to the component-URL shape."""
    for f in page.frames:
        try:
            if f.query_selector("#cb-result") is not None:
                return f
        except Exception:
            pass
    return None


def _wait_frame(page, pred, what: str, timeout_ms: int = 60_000, step_ms: int = 100):
    """Poll until `pred(frame)` holds on the component iframe; return the frame."""
    waited = 0
    while waited < timeout_ms:
        f = _cb_frame(page)
        if f is not None:
            try:
                if pred(f):
                    return f
            except Exception:
                pass
        page.wait_for_timeout(step_ms)
        waited += step_ms
    raise AssertionError(f"callback iframe never reached: {what}")


def _wait_cb_ready(page, timeout_ms: int = 60_000):
    return _wait_frame(page, lambda f: f.get_attribute("#cb-result", "data-ready") == "1",
                       "ready (data-ready=1)", timeout_ms)


def _held(frame) -> bool:
    return bool(frame.evaluate("window.__pythscribe_test && window.__pythscribe_test.held === true"))


def _wait_held(page, timeout_ms: int = 60_000):
    """The barrier window: data-ready == 0 AND the instantiate is parked on the release barrier."""
    return _wait_frame(page, lambda f: f.get_attribute("#cb-result", "data-ready") == "0" and _held(f),
                       "the held (loading, barrier-parked) window", timeout_ms)


def _release(frame) -> bool:
    """Manually release the current barrier. False means NO barrier was current -- i.e. it was
    already released (by hand, or by the TEST_BARRIER_BACKSTOP_MS backstop if the flow outlived it)."""
    return bool(frame.evaluate("window.__pythscribe_test_release()"))


def _drag_iframe(frame, x: float):
    frame.eval_on_selector(
        "#cb-slider",
        "(el, v) => { el.value = String(v); el.dispatchEvent(new Event('input', { bubbles: true })); }",
        x,
    )


def _slider_value(frame) -> str:
    return frame.eval_on_selector("#cb-slider", "el => el.value")


def _read_count(page):
    el = page.query_selector('[data-testid="server-rerun-count"]')
    assert el is not None, "server-rerun-count marker missing"
    return int(el.inner_text())


def _rerun_frame_counter(page):
    """Count client->server `rerun_script` BackMsg WS frames (S-2/S-14). Registered BEFORE goto by
    the caller. Server->client frames and unparseable frames do not count."""
    from streamlit.proto.BackMsg_pb2 import BackMsg

    state = {"n": 0}

    def on_frame(payload):
        try:
            data = payload if isinstance(payload, (bytes, bytearray)) else payload.encode("latin-1")
            m = BackMsg.FromString(data)
            if m.WhichOneof("type") == "rerun_script":
                state["n"] += 1
        except Exception:
            pass

    page.on("websocket", lambda ws: ws.on("framesent", on_frame))
    return state


def _select_kernel(page, name: str):
    """Pick an option of the `flagship kernel` st.selectbox (BaseWeb select)."""
    page.locator('[data-testid="stSelectbox"]').first.click()
    page.get_by_role("option", name=name, exact=True).click()


def _nudge_native_slider(page):
    """Move the NATIVE `st.slider` one step right. Streamlit >= 1.64 renders the slider with
    react-aria: the `role=slider` element is a VISUALLY-HIDDEN `<input type=range>` (1x1 px,
    clipped) under a styled track, so a pointer `click()` on it never lands (Playwright retries the
    hit-target check forever). Keyboard focus reaches it regardless of the DOM generation (the old
    BaseWeb `div[role=slider]` thumb is focusable too), and ArrowRight is the slider's own step."""
    slider = page.get_by_role("slider").first  # only the native slider is in the main document
    slider.focus()
    page.keyboard.press("ArrowRight")


def _pick_mode(page, name: str):
    """Click a `mode` st.radio option of the witness app."""
    page.locator('[data-testid="stRadio"]').get_by_text(name, exact=True).click()


def _assert_drag_sequence(frame, page, xs=(1.0, 3.5, 7.2, 9.9, 0.3), ref=oracle):
    """codex B1 -- per drag, in this order: (1) the slider's ACTUAL DOM value moved to the requested
    x; (2) the CONSUMED input `data-x-bits` equals the bits of that actual value (the handler
    recomputed on THIS input, not a stale one); (3) the result equals the oracle of THAT value.
    A handler that never recomputes fails (2) on the first drag (see the paired mutant)."""
    prev = float(_slider_value(frame))  # the DOM value BEFORE the first drag: every drag, incl. the first, must move it
    for x in xs:
        assert abs(x - prev) > 1e-9, f"requested x={x} equals the current slider value; the drag would not be a change"
        _drag_iframe(frame, x)
        page.wait_for_timeout(120)
        actual = float(_slider_value(frame))
        assert abs(actual - x) < 1e-9, f"drag to {x} did not move the slider (DOM value {actual})"
        assert actual != prev, f"input did not change across drags ({actual})"
        xbits = frame.get_attribute("#cb-result", "data-x-bits")
        assert xbits == bits_of(actual), \
            f"consumed input {xbits and unbits(xbits)} != the slider's actual value {actual} (stale compute)"
        got = frame.get_attribute("#cb-result", "data-bits")
        assert got == bits_of(ref(actual)), f"x={actual}: bits {got} != oracle {bits_of(ref(actual))}"
        assert frame.get_attribute("#cb-result", "data-path") == "browser-wasm"
        prev = actual


# ---- the witness app + component mutants -----------------------------------------------------

_WITNESS_KERNELS = '''\
"""Test-owned kernels for the witness app (built per test by `build_module`; the CPython oracles
`oracle` / `oracle_alt` in test_streamlit_callback_e2e.py mirror these bodies exactly)."""
from pythscribe import wasm


@wasm
def response(x: float) -> float:
    return (x * x) * 0.1 - x + 3.0


@wasm
def response_alt(x: float) -> float:
    # differs from `response` at every x (B3-r: a kernel swap must never leave A's value under B)
    return (x * x) * 0.1 - x + 3.0 + 1.0
'''

_WITNESS_APP = '''\
"""Test-authored WITNESS app (v0.2.5 codex fix round) = the M0 demo + a `mode` radio that switches
the SAME keyed iframe between the callback and preprocessing kinds (SF-4) + an env-selected corrupt
"broken" kernel whose load is parked on the release barrier (B2) + an env-selected component dir
(the E2E mutants serve a MUTATED copy of the vendored component). The kernels + artifacts are
TEST-OWNED (`witness_kernels.py`, built beside this app); only the rerun counter is imported from
the real example dir."""
import os
import sys
from pathlib import Path

import streamlit as st

sys.path.insert(0, os.environ["PYTHSCRIBE_TEST_ORIG_DIR"])
sys.path.insert(0, str(Path(__file__).resolve().parent))
import counter  # noqa: E402
from witness_kernels import response, response_alt  # noqa: E402

import pythscribe.streamlit as S  # noqa: E402
from pythscribe.streamlit import (  # noqa: E402
    CallbackSpec, WasmComponent, client_callback, describe, dispatch, result_of, slider_compute,
)

_cdir = os.environ.get("PYTHSCRIBE_TEST_COMPONENT_DIR")
if _cdir:
    S._COMPONENT_DIR = Path(_cdir)  # E2E mutant: the MUTATED copy of the vendored component
hold = os.environ.get("PYTHSCRIBE_TEST_HOLD", "").strip() not in ("", "0", "false", "no")
corrupt = os.environ.get("PYTHSCRIBE_TEST_CORRUPT_WASM")

runs = counter.bump()
st.html(f'<span data-testid="server-rerun-count">{runs}</span>')
st.checkbox("unrelated toggle (reruns the script; the iframe must stay put)", key="unrelated")
mode = st.radio("mode", ["callback", "preprocessing"], key="mode", horizontal=True)
options = (["broken"] if corrupt else []) + ["response", "response_alt"]
choice = st.selectbox("flagship kernel", options, key="kernel_choice")

if mode == "callback":
    if choice == "broken":
        good = client_callback(response, shape="node")
        spec = CallbackSpec(fn=good.fn, wasm=None, param_types=list(good.param_types),
                            return_type=good.return_type, shape="node",
                            source_sha256="corrupt-" + good.source_sha256, render=None, wasm_path=corrupt)
        slider_compute(spec, min=0.0, max=10.0, step=0.1, key="flagship", label="f(x) [broken]", _test_hold=True)
    else:
        kernel = response if choice == "response" else response_alt
        slider_compute(client_callback(kernel, shape="node"), min=0.0, max=10.0, step=0.1,
                       key="flagship", label=f"f(x) [{choice}]", _test_hold=hold)
else:
    payload = st.session_state.get("pre_payload")
    if payload is None:
        payload = st.session_state["pre_payload"] = dispatch(response_alt, 4.0)
    value = WasmComponent()(payload=payload, key="flagship")
    r = result_of(response_alt, value, expect_nonce=payload["nonce"])
    st.html(f'<span data-testid="pre-result">{describe(r)}</span>')
'''

# Exact substrings of the vendored index.html each mutant removes/replaces. The mutant builder
# asserts the anchor is PRESENT (else the control would be vacuous against a drifted file).
MUT_B1_NO_RECOMPUTE_ON_INPUT = (
    "rerun.\n    renderValue(Number(cbSlider.value));\n  });",
    "rerun.\n  });",
)
MUT_B2_NO_REJECTION_GUARD = (
    "      if (seq !== cbRenderSeq) return;               // B2: a superseded load's REJECTION is ignored too\n",
    "",
)
MUT_SF3_NO_CLEAR_ON_IDENTITY_CHANGE = (
    "    clearResultDisplay();                             // SF-3: no stale result under the new label\n",
    "",
)
MUT_SF4_NO_DEACTIVATE_ON_KIND_SWITCH = (
    "    if (!cbEl.hidden) deactivateCallback();          // SF-4: a kind switch retires the callback UI\n",
    "",
)
MUT_SF6_RESOLVED_ONLY_CACHE = (
    "    kernelPromises.set(sha, pr);\n",
    "    pr.then(() => kernelPromises.set(sha, pr));   // MUTANT: cache only once RESOLVED\n",
)
# codex r2 should-fix: the NARROW mutant -- deactivateCallback keeps the hide/disable but drops ONLY
# the supersession bump, so a PENDING callback load completes late and re-arms the retired slider.
MUT_SF4_NO_SEQ_BUMP_ON_DEACTIVATE = (
    "  function deactivateCallback() {\n    cbRenderSeq += 1;\n",
    "  function deactivateCallback() {\n",
)
# The barrier BACKSTOP (index.html TEST_BARRIER_BACKSTOP_MS): a held barrier auto-releases only after
# this long. It exceeds every per-wait budget below (60 s), so a flow that parks A, drives B ready and
# releases A by hand (the B2 pair) can never be auto-released mid-flow on a slow machine -- it would
# fail its own wait first. Mirrored here and drift-checked against the file (test_barrier_backstop_*).
TEST_BARRIER_BACKSTOP_MS = 120_000
# codex r2 nit: the old UNBOUND auto-release timer (fires window.__pythscribe_test_release() for
# whatever barrier is current) -- a barrier released by hand leaves a stale timer that releases a
# NEWER barrier early. The mutant injects that unbound timer at a SHORT, explicit interval so the
# stale-timer class is observable inside the test budget (the real backstop is 120 s).
BARRIER_MUTANT_TIMER_MS = 15_000
MUT_BARRIER_UNBOUND_TIMER = (
    "      b.timer = setTimeout(() => releaseBarrier(b), TEST_BARRIER_BACKSTOP_MS); // THIS barrier only: never wedge the iframe\n",
    f"      setTimeout(() => window.__pythscribe_test_release(), {BARRIER_MUTANT_TIMER_MS}); // MUTANT: unbound timer\n",
)


def _mutant_component(tmp_path: Path, mutation: tuple[str, str] | None) -> Path:
    """A copy of the vendored component dir with ONE mutation applied to index.html (None = a
    faithful copy, the control that the override itself changes nothing)."""
    d = tmp_path / "component"
    d.mkdir()
    shutil.copy(COMPONENT_DIR / "list_buffer.mjs", d / "list_buffer.mjs")
    html = (COMPONENT_DIR / "index.html").read_text(encoding="utf-8")
    if mutation is not None:
        old, new = mutation
        assert html.count(old) == 1, f"mutant anchor must occur exactly once (found {html.count(old)}): {old!r}"
        html = html.replace(old, new, 1)
        assert html != (COMPONENT_DIR / "index.html").read_text(encoding="utf-8")
    (d / "index.html").write_text(html, encoding="utf-8")
    return d


def _witness_app(tmp_path: Path, *, mutation=None, hold: bool = False, corrupt: bool = False):
    """(app_dir, env) for a StreamlitApp running the witness app."""
    from pythscribe.build import build_module

    app_dir = tmp_path / "witness"
    app_dir.mkdir()
    (app_dir / "app.py").write_text(_WITNESS_APP, encoding="utf-8")
    kernels_py = app_dir / "witness_kernels.py"
    kernels_py.write_text(_WITNESS_KERNELS, encoding="utf-8")
    # the explicit build step (the documented path): lays `__pythscribe__/` beside the module so the
    # app's `client_callback` / `dispatch` bind the artifacts at import; a missing compiler raises
    # here (loud), never a silent Python fallback inside the app.
    built = {a.function for a in build_module(kernels_py, quiet=True)}
    assert built == {"response", "response_alt"}, built
    env = {"PYTHSCRIBE_TEST_ORIG_DIR": str(CALLBACK_DIR)}
    if mutation is not None or corrupt:
        env["PYTHSCRIBE_TEST_COMPONENT_DIR"] = str(_mutant_component(tmp_path, mutation))
    if hold:
        env["PYTHSCRIBE_TEST_HOLD"] = "1"
    if corrupt:
        bad = tmp_path / "corrupt.wasm"
        bad.write_bytes(b"\x00asm" + b"\x01\x00\x00\x00" + b"\xff" * 64)  # valid magic, invalid module
        env["PYTHSCRIBE_TEST_CORRUPT_WASM"] = str(bad)
    return app_dir, env


# ---- M0 GO: the in-iframe slider recomputes in-tab with ZERO server rerun -------------------

def test_m0_go_zero_rerun_and_value_equals_cpython(browser, demo_oracle):
    with StreamlitApp(CALLBACK_DIR) as app:
        ctx = browser.new_context()
        page = ctx.new_page()
        errors: list[str] = []
        page.on("pageerror", lambda e: errors.append(str(e)))
        rr = _rerun_frame_counter(page)
        page.goto(app.url, wait_until="load")
        frame = _wait_cb_ready(page)

        # instantiate-once (positive; the barrier-window control is test_m0_instantiate_once_under_hold)
        assert frame.get_attribute("#cb-result", "data-instantiations") == "1"
        assert frame.get_attribute("#cb-result", "data-path") == "browser-wasm"
        assert frame.get_attribute("#cb-result", "data-wasm-export") == "response"
        assert frame.get_attribute("#cb-result", "data-wasm-how") == "bytes"

        count_before = _read_count(page)
        frames_before = rr["n"]

        # N synthetic drags -> each recomputes IN-TAB on the ACTUAL slider value; == CPython bit-exact
        # (the demo's OWN plain-Python twin of its @wasm kernel is the reference, see `demo_oracle`)
        _assert_drag_sequence(frame, page, ref=demo_oracle)

        page.wait_for_timeout(400)  # settle: any (nonexistent) rerun would have landed
        count_after = _read_count(page)
        frames_after = rr["n"]
        ctx.close()

    # THE MARKER TRIPLE across the whole drag sequence:
    assert count_after - count_before == 0, f"server reruns fired on a drag: {count_before}->{count_after}"
    assert frames_after - frames_before == 0, f"rerun_script frames fired on a drag: delta {frames_after - frames_before}"
    assert errors == [], f"page errors during drag: {errors}"


def test_witness_positive_drag_sequence_recomputes(browser, tmp_path):
    """The WITNESS-app baseline for the B1 control (codex review, anti-vacuity): the UNMUTATED
    witness app (mutation=None, the witness kernels + their own CPython oracle `oracle`) passes the
    full `_assert_drag_sequence`, so `test_b1_mutant_no_recompute_on_input_goes_red` is exactly ONE
    mutation (MUT_B1_NO_RECOMPUTE_ON_INPUT) away from a PASSING positive on the SAME app. Without
    this, a witness app that failed to recompute for any unrelated reason would still turn the B1
    mutant "red for the wrong reason" inside its `pytest.raises`."""
    app_dir, env = _witness_app(tmp_path)
    with StreamlitApp(app_dir, env=env) as app:
        ctx = browser.new_context()
        page = ctx.new_page()
        errors: list[str] = []
        page.on("pageerror", lambda e: errors.append(str(e)))
        page.goto(app.url, wait_until="load")
        frame = _wait_cb_ready(page)
        assert frame.get_attribute("#cb-result", "data-path") == "browser-wasm"
        assert frame.get_attribute("#cb-result", "data-wasm-export") == "response"
        _assert_drag_sequence(frame, page)  # ref=oracle: the witness kernel's own CPython reference
        ctx.close()
    assert errors == [], f"page errors during drag: {errors}"


def test_b1_mutant_no_recompute_on_input_goes_red(browser, tmp_path):
    """PAIRED CONTROL (codex B1): the input listener's `renderValue(...)` removed -> every drag moves
    the DOM slider but the CONSUMED input (`data-x-bits`) stays at the initial x, so the drag
    sequence goes RED at the consumed==actual check (the OLD test, which fed `data-x-bits` to the
    oracle without checking it moved, stayed GREEN on this mutant)."""
    app_dir, env = _witness_app(tmp_path, mutation=MUT_B1_NO_RECOMPUTE_ON_INPUT)
    with StreamlitApp(app_dir, env=env) as app:
        ctx = browser.new_context()
        page = ctx.new_page()
        page.goto(app.url, wait_until="load")
        frame = _wait_cb_ready(page)
        initial_xbits = frame.get_attribute("#cb-result", "data-x-bits")
        with pytest.raises(AssertionError, match="consumed input|did not change"):
            _assert_drag_sequence(frame, page)
        # the RED is for the right reason: the slider moved, the consumed input did not
        assert float(_slider_value(frame)) == 1.0
        assert frame.get_attribute("#cb-result", "data-x-bits") == initial_xbits
        ctx.close()


# ---- The PAIRED control that makes "no rerun" non-vacuous: native st.slider trips the counters --

def test_m0_native_control_trips_counter(browser):
    with StreamlitApp(CALLBACK_DIR) as app:
        ctx = browser.new_context()
        page = ctx.new_page()
        rr = _rerun_frame_counter(page)
        page.goto(app.url, wait_until="load")
        _wait_cb_ready(page)

        count_before = _read_count(page)
        frames_before = rr["n"]
        # drive the NATIVE st.slider -> server recompute -> a full script rerun
        _nudge_native_slider(page)
        # wait for the rerun to land (the counter to move)
        deadline = 0
        while deadline < 15_000:
            if _read_count(page) > count_before:
                break
            page.wait_for_timeout(200)
            deadline += 200
        count_after = _read_count(page)
        frames_after = rr["n"]
        ctx.close()

    assert count_after - count_before > 0, "native-slider drag did NOT rerun the script (control vacuous)"
    assert frames_after - frames_before > 0, "native-slider drag sent no rerun_script frame (control vacuous)"


# ---- Instantiate-once (B4-r / codex SF-6): a same-sha re-render that PROVABLY overlaps the load ---

def _overlapping_rerender_then_release(browser, app_dir, env):
    """Drive the release-barrier window: park the first instantiate on the barrier, toggle the
    unrelated checkbox (a same-sha re-render) and PROVE it arrived WHILE held
    (`rendersWhileHeld >= 1` with `held` still true), drag (a pre-ready no-op), then release.
    Returns (data-instantiations, pageerrors)."""
    with StreamlitApp(app_dir, env=env) as app:
        ctx = browser.new_context()
        page = ctx.new_page()
        errors: list[str] = []
        page.on("pageerror", lambda e: errors.append(str(e)))
        page.goto(app.url, wait_until="load")

        frame = _wait_held(page)
        assert frame.get_attribute("#cb-result", "data-ready") == "0"

        # a same-sha re-render mid-instantiate: toggle the unrelated checkbox by clicking its
        # visible label (Streamlit positions the real <input> off-screen behind a styled label)
        page.get_by_text("unrelated toggle", exact=False).first.click()
        # the OVERLAP, proven: the re-render was counted while the barrier was still held
        frame = _wait_frame(
            page,
            lambda f: _held(f) and int(f.evaluate("window.__pythscribe_test.rendersWhileHeld")) >= 1,
            "a re-render counted WHILE the instantiate was held", 30_000,
        )
        assert frame.get_attribute("#cb-result", "data-ready") == "0"  # still loading
        # a drag before ready must be a no-op (never a TypeError / pageerror)
        _drag_iframe(frame, 5.0)
        page.wait_for_timeout(200)
        assert frame.get_attribute("#cb-result", "data-bits") is None

        assert _release(frame)  # now let it finish
        frame = _wait_cb_ready(page)
        page.wait_for_timeout(300)
        insts = frame.get_attribute("#cb-result", "data-instantiations")
        ctx.close()
    return insts, errors


def test_m0_instantiate_once_under_hold(browser, tmp_path):
    """A promise cache instantiates ONCE under a PROVEN-overlapping same-sha re-render
    (`data-instantiations == 1`). Paired: test_sf6_mutant_resolved_cache_double_instantiates.
    Driven on the FAITHFUL witness copy (mutation=None -- the same app the mutant runs, one
    mutation apart): the re-render trigger is the witness app's `unrelated toggle` checkbox, which
    the polished demo app no longer carries."""
    app_dir, env = _witness_app(tmp_path, hold=True)
    insts, errors = _overlapping_rerender_then_release(browser, app_dir, env)
    assert insts == "1", f"instantiated {insts} times under a same-sha mid-instantiate re-render (expected 1)"
    assert errors == [], f"drag-before-ready or re-render raised: {errors}"


def test_sf6_mutant_resolved_cache_double_instantiates(browser, tmp_path):
    """PAIRED CONTROL (codex SF-6): a cache of the RESOLVED kernel (not the promise) starts a second
    instantiate for the overlapping re-render -> `data-instantiations == 2` -> the positive test's
    `== 1` goes RED. (The old timed-sleep control could not establish the overlap, so a resolved-only
    cache still reported 1 and passed.)"""
    app_dir, env = _witness_app(tmp_path, mutation=MUT_SF6_RESOLVED_ONLY_CACHE, hold=True)
    insts, _ = _overlapping_rerender_then_release(browser, app_dir, env)
    # one instantiate per render that arrived while held (observed: 3 = the initial + the toggle's
    # renders); ANY count above 1 is the failure the positive test's `== 1` guards
    assert int(insts) >= 2, f"resolved-cache mutant must re-instantiate under a proven overlap (got {insts})"


# ---- codex B2: a superseded FAILING load must not clobber a newer valid load ------------------

def _late_reject_after_newer_success(browser, tmp_path, mutation):
    """Witness: load A ("broken": corrupt bytes, parked on the barrier) -> switch to B ("response",
    resolves + renders) -> release A -> A's instantiate REJECTS after B succeeded. Returns the
    post-rejection observables of #cb-result."""
    import time
    app_dir, env = _witness_app(tmp_path, mutation=mutation, corrupt=True)
    with StreamlitApp(app_dir, env=env) as app:
        ctx = browser.new_context()
        page = ctx.new_page()
        page.goto(app.url, wait_until="load")
        frame = _wait_held(page)                     # A: parked, failing once released
        t_parked = time.monotonic()
        assert frame.text_content("#cb-label") == "f(x) [broken]"

        _select_kernel(page, "response")             # B: a newer valid identity
        frame = _wait_cb_ready(page)
        assert frame.text_content("#cb-label") == "f(x) [response]"
        x = unbits(frame.get_attribute("#cb-result", "data-x-bits"))
        b_bits = frame.get_attribute("#cb-result", "data-bits")
        assert b_bits == bits_of(oracle(x))

        # A must STILL be parked: the flow above is bounded by its own waits (<= 60 s), well inside
        # the 120 s backstop, so the manual release below is the ONLY release A gets.
        held_for = time.monotonic() - t_parked
        assert held_for < TEST_BARRIER_BACKSTOP_MS / 1000 - 10, f"flow outlived the barrier backstop ({held_for:.1f}s)"
        assert _held(frame), f"A was auto-released before the manual release ({held_for:.1f}s after parking)"
        assert _release(frame)                       # A now instantiates the corrupt bytes -> rejects
        page.wait_for_timeout(600)
        obs = {
            "ready": frame.get_attribute("#cb-result", "data-ready"),
            "error": frame.get_attribute("#cb-result", "data-error"),
            "bits": frame.get_attribute("#cb-result", "data-bits"),
            "status": frame.text_content("#status"),
            "disabled": frame.eval_on_selector("#cb-slider", "el => el.disabled"),
        }
        # an unchanged-B rerender (unrelated toggle) takes the identity shortcut: it must not
        # resurrect anything either
        page.get_by_text("unrelated toggle", exact=False).first.click()
        page.wait_for_timeout(600)
        frame = _cb_frame(page)
        obs["error_after_rerender"] = frame.get_attribute("#cb-result", "data-error")
        obs["bits_after_rerender"] = frame.get_attribute("#cb-result", "data-bits")
        # B stays operational?
        _drag_iframe(frame, 2.5)
        page.wait_for_timeout(120)
        obs["drag_bits"] = frame.get_attribute("#cb-result", "data-bits")
        obs["drag_x"] = frame.get_attribute("#cb-result", "data-x-bits")
        ctx.close()
    obs["b_bits"] = b_bits
    return obs


def test_b2_superseded_rejection_does_not_clobber_newer_success(browser, tmp_path):
    obs = _late_reject_after_newer_success(browser, tmp_path, None)
    assert obs["error"] is None, f"late rejection of superseded A clobbered B: {obs}"
    assert obs["bits"] == obs["b_bits"] and obs["ready"] == "1" and obs["status"] == "ready", obs
    assert obs["disabled"] is False
    assert obs["error_after_rerender"] is None and obs["bits_after_rerender"] == obs["b_bits"], obs
    assert obs["drag_x"] == bits_of(2.5) and obs["drag_bits"] == bits_of(oracle(2.5)), obs


def test_b2_mutant_no_rejection_guard_clobbers(browser, tmp_path):
    """PAIRED CONTROL (codex B2): with the rejection-branch supersession guard removed, the late
    A-rejection overwrites B's result with error:LoadError (bits cleared, data-ready still 1), and
    the unchanged-B rerender keeps the stale error -> the positive test goes RED."""
    obs = _late_reject_after_newer_success(browser, tmp_path, MUT_B2_NO_REJECTION_GUARD)
    assert obs["error"] == "LoadError" and obs["bits"] is None and obs["ready"] == "1", obs
    assert obs["error_after_rerender"] == "LoadError", obs


# ---- codex SF-3: no stale result under a NEW label while the new kernel loads -----------------

def _swap_kernel_during_hold(browser, tmp_path, mutation):
    """Witness: ready on `response`, drag to 3.5, swap to `response_alt` (new sha -> parked on the
    barrier). Returns the observables DURING the hold and after release."""
    app_dir, env = _witness_app(tmp_path, mutation=mutation, hold=True)
    with StreamlitApp(app_dir, env=env) as app:
        ctx = browser.new_context()
        page = ctx.new_page()
        page.goto(app.url, wait_until="load")
        frame = _wait_held(page)
        assert _release(frame)
        frame = _wait_cb_ready(page)
        _drag_iframe(frame, 3.5)
        page.wait_for_timeout(120)
        a_text = frame.text_content("#cb-result")
        assert frame.get_attribute("#cb-result", "data-bits") == bits_of(oracle(3.5))

        _select_kernel(page, "response_alt")
        frame = _wait_held(page)
        during = {
            "label": frame.text_content("#cb-label"),
            "bits": frame.get_attribute("#cb-result", "data-bits"),
            "xbits": frame.get_attribute("#cb-result", "data-x-bits"),
            "text": frame.text_content("#cb-result"),
            "stale": frame.get_attribute("#cb-stale", "data-stale"),
            "ready": frame.get_attribute("#cb-result", "data-ready"),
            "disabled": frame.eval_on_selector("#cb-slider", "el => el.disabled"),
        }
        assert _release(frame)
        frame = _wait_cb_ready(page)
        page.wait_for_timeout(120)
        after = {
            "xbits": frame.get_attribute("#cb-result", "data-x-bits"),
            "bits": frame.get_attribute("#cb-result", "data-bits"),
            "stale": frame.get_attribute("#cb-stale", "data-stale"),
        }
        ctx.close()
    return a_text, during, after


def test_sf3_result_cleared_on_identity_change(browser, tmp_path):
    a_text, during, after = _swap_kernel_during_hold(browser, tmp_path, None)
    assert during["label"] == "f(x) [response_alt]", during
    assert during["bits"] is None and during["xbits"] is None, f"A's result persisted under B's label: {during}"
    assert during["text"] == "loading" and during["ready"] == "0" and during["disabled"] is True, during
    assert during["stale"] == a_text, during                      # last-good moved to data-stale
    x = unbits(after["xbits"])
    assert x == 3.5 and after["bits"] == bits_of(oracle_alt(x)) and after["stale"] is None, after


def test_sf3_mutant_no_clear_keeps_stale_result(browser, tmp_path):
    """PAIRED CONTROL (codex SF-3): without the clear, A's bits/text sit under B's label for the
    whole load -> the positive test goes RED."""
    a_text, during, _ = _swap_kernel_during_hold(browser, tmp_path, MUT_SF3_NO_CLEAR_ON_IDENTITY_CHANGE)
    assert during["label"] == "f(x) [response_alt]"
    assert during["bits"] == bits_of(oracle(3.5)) and during["text"] == a_text, during


# ---- codex SF-4: a kind switch on the same keyed iframe deactivates the callback UI -----------

def _switch_kind_on_keyed_iframe(browser, tmp_path, mutation):
    app_dir, env = _witness_app(tmp_path, mutation=mutation)
    with StreamlitApp(app_dir, env=env) as app:
        ctx = browser.new_context()
        page = ctx.new_page()
        errors: list[str] = []
        page.on("pageerror", lambda e: errors.append(str(e)))
        page.goto(app.url, wait_until="load")
        frame = _wait_cb_ready(page)
        _drag_iframe(frame, 2.0)
        page.wait_for_timeout(120)
        assert frame.get_attribute("#cb-result", "data-bits") == bits_of(oracle(2.0))

        _pick_mode(page, "preprocessing")            # SAME key -> the same iframe, a preprocessing payload
        # the preprocessing result lands (setComponentValue -> rerun -> result_of)
        deadline = 0
        while deadline < 30_000:
            el = page.query_selector('[data-testid="pre-result"]')
            if el is not None and "path=browser-wasm" in el.inner_text():
                break
            page.wait_for_timeout(200)
            deadline += 200
        pre = page.query_selector('[data-testid="pre-result"]').inner_text()
        frame = _cb_frame(page)
        page.wait_for_timeout(300)
        obs = {
            "pre": pre,
            "status": frame.text_content("#status"),
            "cb_hidden": frame.eval_on_selector("#cb", "el => el.hidden"),
            "disabled": frame.eval_on_selector("#cb-slider", "el => el.disabled"),
            "ready": frame.get_attribute("#cb-result", "data-ready"),
            "bits": frame.get_attribute("#cb-result", "data-bits"),
        }
        _drag_iframe(frame, 5.0)                     # a drag on the retired slider must compute NOTHING
        page.wait_for_timeout(200)
        obs["bits_after_drag"] = frame.get_attribute("#cb-result", "data-bits")

        _pick_mode(page, "callback")                 # and back: the callback UI returns, ready
        frame = _wait_cb_ready(page)
        page.wait_for_timeout(120)
        obs["back_hidden"] = frame.eval_on_selector("#cb", "el => el.hidden")
        obs["back_pre_hidden"] = frame.eval_on_selector("#payload", "el => el.hidden")
        x = unbits(frame.get_attribute("#cb-result", "data-x-bits"))
        obs["back_ok"] = frame.get_attribute("#cb-result", "data-bits") == bits_of(oracle(x))
        obs["insts"] = frame.get_attribute("#cb-result", "data-instantiations")
        ctx.close()
    obs["errors"] = errors
    return obs


def test_sf4_kind_switch_deactivates_callback_ui(browser, tmp_path):
    obs = _switch_kind_on_keyed_iframe(browser, tmp_path, None)
    assert "path=browser-wasm" in obs["pre"] and obs["status"].startswith("done (browser-wasm)"), obs
    assert obs["cb_hidden"] is True and obs["disabled"] is True and obs["ready"] == "0", obs
    assert obs["bits"] is None and obs["bits_after_drag"] is None, f"retired slider still computes: {obs}"
    assert obs["back_hidden"] is False and obs["back_pre_hidden"] is True and obs["back_ok"], obs
    assert obs["insts"] == "1", obs                  # the promise cache survives the kind round-trip
    assert obs["errors"] == [], obs


def test_sf4_mutant_kind_switch_leaves_callback_active(browser, tmp_path):
    """PAIRED CONTROL (codex SF-4): without the deactivation, A's slider stays visible AND
    operational beside B's preprocessing output -> the positive test goes RED."""
    obs = _switch_kind_on_keyed_iframe(browser, tmp_path, MUT_SF4_NO_DEACTIVATE_ON_KIND_SWITCH)
    assert "path=browser-wasm" in obs["pre"], obs
    assert obs["cb_hidden"] is False and obs["disabled"] is False and obs["ready"] == "1", obs
    assert obs["bits_after_drag"] == bits_of(oracle(5.0)), f"mutant slider must still compute: {obs}"


def _pending_load_kind_switch(browser, tmp_path, mutation):
    """codex r2 should-fix witness: callback A is PENDING (parked on the barrier) when the same key
    switches to preprocessing; A is released only AFTER the preprocessing result landed. Returns
    the retired slider's observables after A's late completion."""
    app_dir, env = _witness_app(tmp_path, mutation=mutation, hold=True)
    with StreamlitApp(app_dir, env=env) as app:
        ctx = browser.new_context()
        page = ctx.new_page()
        errors: list[str] = []
        page.on("pageerror", lambda e: errors.append(str(e)))
        page.goto(app.url, wait_until="load")
        frame = _wait_held(page)                     # A: pending, not ready
        assert frame.get_attribute("#cb-result", "data-ready") == "0"

        _pick_mode(page, "preprocessing")            # kind switch WHILE A is still loading
        deadline = 0
        while deadline < 30_000:
            el = page.query_selector('[data-testid="pre-result"]')
            if el is not None and "path=browser-wasm" in el.inner_text():
                break
            page.wait_for_timeout(200)
            deadline += 200
        frame = _cb_frame(page)
        assert "path=browser-wasm" in page.query_selector('[data-testid="pre-result"]').inner_text()
        assert frame.eval_on_selector("#cb", "el => el.hidden") is True
        assert _held(frame)                          # A is STILL parked (the switch did not release it)

        assert _release(frame)                       # A's load now completes LATE, under preprocessing
        page.wait_for_timeout(600)
        obs = {
            "ready": frame.get_attribute("#cb-result", "data-ready"),
            "disabled": frame.eval_on_selector("#cb-slider", "el => el.disabled"),
            "bits": frame.get_attribute("#cb-result", "data-bits"),
            "hidden": frame.eval_on_selector("#cb", "el => el.hidden"),
            "status": frame.text_content("#status"),
        }
        _drag_iframe(frame, 5.0)                     # synthetic input on the retired slider
        page.wait_for_timeout(200)
        obs["bits_after_drag"] = frame.get_attribute("#cb-result", "data-bits")
        ctx.close()
    obs["errors"] = errors
    return obs


def test_sf4_pending_load_is_invalidated_by_kind_switch(browser, tmp_path):
    """A's late completion must be invalidated by the seq bump: the retired slider never becomes
    enabled/ready and never accepts synthetic input."""
    obs = _pending_load_kind_switch(browser, tmp_path, None)
    assert obs["hidden"] is True and obs["disabled"] is True and obs["ready"] == "0", obs
    assert obs["bits"] is None and obs["bits_after_drag"] is None, f"retired slider re-armed by a late load: {obs}"
    assert obs["status"].startswith("done (browser-wasm)"), obs   # the preprocessing status was not overwritten
    assert obs["errors"] == [], obs


def test_sf4_mutant_no_seq_bump_rearms_retired_slider(browser, tmp_path):
    """PAIRED CONTROL (narrow): deactivateCallback WITHOUT the seq bump (hide/disable kept) -> A's
    late completion passes the supersession check, re-enables the hidden slider, marks it ready,
    renders a value and accepts input -> the positive test goes RED. (The already-ready SF-4
    witness above cannot see this: its A had completed before the switch.)"""
    obs = _pending_load_kind_switch(browser, tmp_path, MUT_SF4_NO_SEQ_BUMP_ON_DEACTIVATE)
    assert obs["hidden"] is True, obs                            # the hide survived -- only the bump is gone
    assert obs["disabled"] is False and obs["ready"] == "1" and obs["bits"] is not None, obs
    assert obs["bits_after_drag"] == bits_of(oracle(5.0)), obs


# ---- codex r2 nit: a barrier's auto-release timer belongs to ITS barrier only -----------------

def _manual_release_then_newer_barrier(browser, tmp_path, mutation):
    """Witness: hold A at page load, release it BY HAND, wait ~8 s, then swap kernels so a NEWER
    barrier B is created; probe at tA + BARRIER_MUTANT_TIMER_MS + 2.5 s -- after the mutant's
    stale A timer would have fired, but before B's OWN timer under the mutant (tB + 15 s >= tA +
    23 s) and far before the real backstop (tA/tB + 120 s), so a release seen here can only come
    from A's stale timer. Returns whether B is still held (+ ready state)."""
    import time
    app_dir, env = _witness_app(tmp_path, mutation=mutation, hold=True)
    with StreamlitApp(app_dir, env=env) as app:
        ctx = browser.new_context()
        page = ctx.new_page()
        page.goto(app.url, wait_until="load")
        frame = _wait_held(page)                     # A's barrier; its timer was armed at or before now
        t_a = time.monotonic()
        assert _release(frame)                       # manual release of A
        frame = _wait_cb_ready(page)
        page.wait_for_timeout(8_000)                 # separate the two timers (B's fires >= t_a + 23 s)
        _select_kernel(page, "response_alt")         # new sha -> a NEWER barrier B
        frame = _wait_held(page)
        assert time.monotonic() - t_a < BARRIER_MUTANT_TIMER_MS / 1000 - 2.0, "B must be armed BEFORE A's timer fires"
        remaining_ms = int((t_a + (BARRIER_MUTANT_TIMER_MS + 2_500) / 1000 - time.monotonic()) * 1000)
        page.wait_for_timeout(max(0, remaining_ms))
        elapsed = time.monotonic() - t_a
        assert elapsed < (BARRIER_MUTANT_TIMER_MS + 6_000) / 1000, "probe must precede B's own (mutant) timer"
        assert elapsed < TEST_BARRIER_BACKSTOP_MS / 1000 - 10, "probe must precede the real backstop"
        obs = {"held": _held(frame), "ready": frame.get_attribute("#cb-result", "data-ready")}
        if obs["held"]:
            assert _release(frame)                   # B releases by hand, exactly once
            frame = _wait_cb_ready(page)
            obs["bits_ok"] = frame.get_attribute("#cb-result", "data-bits") == bits_of(
                oracle_alt(unbits(frame.get_attribute("#cb-result", "data-x-bits"))))
        ctx.close()
    return obs


def test_barrier_backstop_mirror_matches_index_html():
    """Drift guard (no browser): the Python mirrors are what the timing math above is calibrated
    to, so the vendored file must declare the SAME backstop, the mutation anchor must be present
    exactly once, and the backstop must exceed every per-wait budget the flows rely on."""
    html = (COMPONENT_DIR / "index.html").read_text(encoding="utf-8")
    assert f"const TEST_BARRIER_BACKSTOP_MS = {TEST_BARRIER_BACKSTOP_MS};" in html
    assert html.count(MUT_BARRIER_UNBOUND_TIMER[0]) == 1
    assert "RUN_TIMEOUT_MS = 15000" in html and TEST_BARRIER_BACKSTOP_MS > 60_000 > BARRIER_MUTANT_TIMER_MS
    assert str(BARRIER_MUTANT_TIMER_MS) in MUT_BARRIER_UNBOUND_TIMER[1]


MUT_BARRIER_BACKSTOP_IS_PRODUCTION_TIMEOUT = (
    "  const TEST_BARRIER_BACKSTOP_MS = 120000;\n",
    "  const TEST_BARRIER_BACKSTOP_MS = RUN_TIMEOUT_MS; // MUTANT: the old shared 15 s backstop\n",
)


def _park_then_wait(browser, tmp_path, mutation, wait_ms: int):
    """Park A on the barrier and simply WAIT `wait_ms` (a slow machine's B2 flow, made explicit);
    return whether A is still held and whether the manual release still succeeds."""
    app_dir, env = _witness_app(tmp_path, mutation=mutation, hold=True)
    with StreamlitApp(app_dir, env=env) as app:
        ctx = browser.new_context()
        page = ctx.new_page()
        page.goto(app.url, wait_until="load")
        frame = _wait_held(page)
        page.wait_for_timeout(wait_ms)
        obs = {"held": _held(frame), "ready": frame.get_attribute("#cb-result", "data-ready"), "released": _release(frame)}
        ctx.close()
    return obs


def test_barrier_survives_a_slow_flow_past_the_production_timeout(browser, tmp_path):
    """The B2-flake fix's own control: a held barrier is NOT auto-released 17 s after parking
    (> RUN_TIMEOUT_MS = 15 s, the production instantiate timeout the backstop used to share), so a
    B2 flow that takes that long on a slow machine still gets its manual release."""
    obs = _park_then_wait(browser, tmp_path, None, 17_000)
    assert obs["held"] is True and obs["ready"] == "0" and obs["released"] is True, obs


def test_barrier_mutant_production_timeout_backstop_auto_releases(browser, tmp_path):
    """PAIRED CONTROL: with the backstop reverted to RUN_TIMEOUT_MS the same 17 s wait finds A
    already auto-released and the manual release returns False -- the exact flake -> RED."""
    obs = _park_then_wait(browser, tmp_path, MUT_BARRIER_BACKSTOP_IS_PRODUCTION_TIMEOUT, 17_000)
    assert obs["held"] is False and obs["ready"] == "1" and obs["released"] is False, obs


def test_barrier_manual_release_does_not_auto_release_newer_barrier(browser, tmp_path):
    obs = _manual_release_then_newer_barrier(browser, tmp_path, None)
    assert obs["held"] is True and obs["ready"] == "0", f"A's stale timer released B early: {obs}"
    assert obs["bits_ok"] is True, obs


def test_barrier_mutant_unbound_timer_releases_newer_barrier(browser, tmp_path):
    """PAIRED CONTROL: the old unbound timer -> A's timer fires after RUN_TIMEOUT_MS and releases
    B (the current barrier) -> B is no longer held / is ready -> the positive test goes RED."""
    obs = _manual_release_then_newer_barrier(browser, tmp_path, MUT_BARRIER_UNBOUND_TIMER)
    assert obs["held"] is False and obs["ready"] == "1", obs
