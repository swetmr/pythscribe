"""V3 (M0 GO gate, spec `12-09-26-lib-gradio-callback-path`): the React + @xyflow/react ISLAND
mounts inside the Svelte-5 Gradio component, a Python `@wasm` kernel (instantiated ONCE) is called
SYNCHRONOUSLY from a ReactFlow custom node's compute, and in headless Chromium the node renders and
the in-tab result equals the CPython oracle. The POSITIVE GO assertion (S3): island mounts + a
stylesheet-only computed property (`.react-flow__node { position: absolute }`, folded from
@xyflow/react/dist/style.css -- NOT the island's explicit height, so it is non-vacuous, S-r2-2) +
node innerText == oracle + the MARKER TRIPLE (path=browser-wasm, server counter delta 0, 0 compute
requests).

Paired negative controls (anti-vacuity):
  * MARKER (B4): the demo's "server round-trip" control wires the SAME kernel through Python; it MUST
    move the server-side counter -> proving `python_calls==0` on the in-tab path is not vacuous.
  * ISLAND ABSENT (S9, not a bypass flag): a scalar (`kind`-less) payload renders NO `.react-flow__node`
    -> proving the mount assertion detects the island's absence.

Gated (skips cleanly) without playwright + chromium, gradio, and the built island templates.
Requires the frontend built with the island (flow_island chunk + xyflow CSS in style.css) and the
pyths compiler (compile-on-first-call for the demo kernel).
"""
from __future__ import annotations

import os
import socket
import subprocess
import sys
import time
import urllib.request
from contextlib import closing
from pathlib import Path

import pytest

from conftest import gate, gate_import

REPO = Path(__file__).resolve().parents[2]
DEMO = REPO / "examples" / "gradio-flow-graph"
BACKEND = REPO / "pythscribe" / "gradio" / "wasm_function" / "backend"
TEMPLATES = BACKEND / "gradio_wasmfunction" / "templates" / "component"

# SF-5: the oracle is the CPython fallback itself (not a hand-typed "6"). `gain`'s Python body run
# under CPython gives a float `6.0`; the in-tab @wasm path renders it as "6" (JS `String(6.0)` ->
# "6": a DISPLAY choice, #493 repr arm), so the fidelity check compares NUMERICALLY (bit-exact for
# an integer-valued float), and the "6" vs "6.0" difference is asserted as the documented display.
def _cpython_oracle_gain() -> float:
    import importlib.util
    src = DEMO / "kernels.py"
    spec = importlib.util.spec_from_file_location("_flowgraph_kernels_oracle", src)
    m = importlib.util.module_from_spec(spec)
    sys.modules["_flowgraph_kernels_oracle"] = m
    spec.loader.exec_module(m)
    from pythscribe.decorators import binding_of
    return binding_of(m.gain).run_python(2.0, 3.0)  # the CPython body, explicitly


# JS-side marker-path recorder: install BEFORE page scripts so `window.__pythscribeFlow = m`
# (Index.svelte's onMarker) is captured as a SEQUENCE -> the "loading"/"error"/"browser-wasm"
# transients are observed reliably (not raced against a route delay).
_MARKER_RECORDER = (
    "(()=>{let s=[];Object.defineProperty(window,'__pythscribeFlow',{configurable:true,"
    "set(v){s.push(v&&v.path);this.__pf=v;},get(){return this.__pf;}});"
    "window.__flowMarkerPaths=s;})();"
)


@pytest.fixture(autouse=True, scope="module")
def _need_stack():
    gate_import("gradio")
    gate_import("playwright.sync_api")
    # the committed templates must carry the built island (chunk + folded CSS), else this gate is
    # about an un-built frontend, not the island itself
    if not TEMPLATES.exists() or not list(TEMPLATES.glob("flow_island-*.js")):
        gate(False, "frontend not built with the island (no flow_island-*.js in templates/component)")
    style = (TEMPLATES / "style.css").read_text(encoding="utf-8")
    gate(".react-flow__node" in style, "xyflow CSS not folded into style.css (rebuild frontend)")


def _free_port() -> int:
    with closing(socket.socket(socket.AF_INET, socket.SOCK_STREAM)) as s:
        s.bind(("127.0.0.1", 0))
        return s.getsockname()[1]


def _app_env(port: int) -> dict[str, str]:
    # Force the WORKTREE's pythscribe + gradio_wasmfunction (a sibling editable install may otherwise
    # win); a Windows-style PYTHONPATH (Python cannot parse POSIX drive paths).
    wt = str(REPO).replace("\\", "/")
    be = str(BACKEND).replace("\\", "/")
    env = {**os.environ, "PYTHONPATH": f"{wt};{be}", "PYTHONUTF8": "1",
           "GRADIO_SERVER_PORT": str(port), "GRADIO_ANALYTICS_ENABLED": "False",
           # compile-on-first-call ON for the demo (the suite's autouse fixture sets NO_JIT=1 in the
           # pytest process; the demo needs a real gain.wasm to serve, so force JIT on here).
           "PYTHSCRIBE_NO_JIT": "0"}
    if "PYTHSCRIBE_PYTHS" not in env:
        # fall back to a sibling release binary if present (CI builds it into target/release)
        for cand in (REPO / "target" / "release" / "pyths.exe", REPO / "target" / "release" / "pyths"):
            if cand.is_file():
                env["PYTHSCRIBE_PYTHS"] = str(cand)
                break
    return env


class _App:
    def __init__(self):
        self.port = _free_port()
        self.url = f"http://127.0.0.1:{self.port}"
        self.proc = None
        self.log = DEMO / "_e2e.log"

    def __enter__(self):
        self._f = open(self.log, "w", encoding="utf-8")
        self.proc = subprocess.Popen([sys.executable, str(DEMO / "app.py")], cwd=str(DEMO),
                                     env=_app_env(self.port), stdout=self._f, stderr=subprocess.STDOUT)
        deadline = time.time() + 120
        while time.time() < deadline:
            if self.proc.poll() is not None:
                self._f.close()
                raise RuntimeError(f"app exited early:\n{self.log.read_text(encoding='utf-8')[-3000:]}")
            try:
                with urllib.request.urlopen(self.url + "/", timeout=2) as r:
                    if r.status == 200:
                        return self
            except Exception:
                time.sleep(0.5)
        self.__exit__(None, None, None)
        raise TimeoutError(f"app did not come up:\n{self.log.read_text(encoding='utf-8')[-3000:]}")

    def __exit__(self, *exc):
        if self.proc and self.proc.poll() is None:
            self.proc.kill()
            try:
                self.proc.wait(timeout=15)
            except Exception:
                pass
        try:
            self._f.close()
        except Exception:
            pass


def _is_compute(u: str) -> bool:
    # requests to /gradio_api/* other than static file= fetches (the probe's _is_compute)
    return "/gradio_api/" in u and "file=" not in u and any(s in u for s in ("/run", "/queue", "/call", "/api/", "/predict"))


@pytest.fixture(scope="module")
def driven():
    from playwright.sync_api import sync_playwright

    with _App() as app:
        with sync_playwright() as p:
            b = p.chromium.launch(headless=True)
            page = b.new_context().new_page()
            reqs: list[str] = []
            errors: list[str] = []
            page.on("request", lambda r: reqs.append(r.url))
            page.on("console", lambda m: errors.append(f"{m.type}: {m.text}") if m.type == "error" else None)
            page.on("pageerror", lambda e: errors.append(f"pageerror: {e}"))
            page.goto(app.url, wait_until="load")
            page.wait_for_selector(".react-flow__node", timeout=60000)
            page.wait_for_function("() => window.__pythscribeFlow && window.__pythscribeFlow.ready === true", timeout=60000)
            page.wait_for_function(
                "() => { const e = document.querySelector('[data-testid=flow-node-value]'); return e && e.textContent && e.textContent !== 'loading'; }",
                timeout=60000)
            info = page.evaluate(r"""() => {
                const node = document.querySelector('.react-flow__node');
                const valEl = document.querySelector('[data-testid=flow-node-value]');
                return {
                    node_present: !!node,
                    node_count: document.querySelectorAll('.react-flow__node').length,
                    react_flow_present: !!document.querySelector('.react-flow'),
                    position: node ? getComputedStyle(node).position : null,
                    value_text: valEl ? valEl.textContent : null,
                    marker: window.__pythscribeFlow || null,
                };
            }""")
            # SNAPSHOT the network markers NOW (after the in-tab compute, BEFORE the deliberate
            # server-round-trip clicks) -- those clicks are compute requests by design.
            wasm_reqs = [u for u in reqs if "gain.wasm" in u]
            compute_reqs = [u for u in reqs if _is_compute(u)]

            def click(sel):
                loc = page.locator(sel)
                if loc.evaluate("el => el.tagName.toLowerCase()") != "button":
                    loc = page.locator(f"{sel} button").first
                loc.click()

            click("#read-count"); page.wait_for_timeout(1000)
            count_before = page.eval_on_selector("#server-count input", "el => el.value")
            click("#server-run"); page.wait_for_timeout(1500)
            click("#read-count"); page.wait_for_timeout(1000)
            count_after = page.eval_on_selector("#server-count input", "el => el.value")

            b.close()
            return {
                "info": info,
                "wasm_reqs": wasm_reqs,
                "compute_reqs": compute_reqs,  # snapshot BEFORE the control clicks
                "count_before": count_before,
                "count_after": count_after,
                "errors": errors,
            }


# ---- V3 GO assertion (positive) ---------------------------------------------------------------

def test_island_mounts_and_renders(driven):
    info = driven["info"]
    assert info["node_present"] and info["react_flow_present"], info
    assert info["node_count"] == 3, f"expected 3 nodes (x, k, gain), got {info['node_count']}"
    assert not driven["errors"], f"page errors: {driven['errors']}"


def test_stylesheet_only_position_absolute(driven):
    # NON-VACUOUS CSS check (S-r2-2): position:absolute is set ONLY by @xyflow/react's stylesheet
    # (folded into style.css), not by the island's explicit container height.
    assert driven["info"]["position"] == "absolute", driven["info"]


def test_node_value_equals_cpython_oracle(driven):
    # SF-5: compare NUMERICALLY to the CPython fallback (bit-exact for an integer-valued float),
    # not to a hand-typed string. The rendered text is the display form.
    oracle = _cpython_oracle_gain()
    assert oracle == 6.0 and isinstance(oracle, float), f"CPython oracle changed: {oracle!r}"
    text = driven["info"]["value_text"]
    assert float(text) == oracle, f"in-tab value {text!r} != CPython oracle {oracle!r}"
    # DOCUMENTED display choice (#493 repr arm): JS renders the float 6.0 as "6" (String(6.0)),
    # while CPython str(6.0) is "6.0". The VALUE is identical (float(text) == 6.0); only the
    # rendering differs -- a display decision, not a fidelity gap.
    assert text == "6", f"expected the '6' display form (6.0 rendered as '6'), got {text!r}"
    assert str(oracle) == "6.0", "CPython str(6.0) is '6.0' (the documented display difference)"
    assert driven["info"]["marker"]["values"] == {"x": "2", "k": "3", "g": "6"}, driven["info"]["marker"]


def test_marker_triple_in_tab(driven):
    # path=browser-wasm AND 0 compute requests AND the .wasm fetched (once)
    assert driven["info"]["marker"]["path"] == "browser-wasm", driven["info"]["marker"]
    assert driven["compute_reqs"] == [], f"in-tab path made compute requests: {driven['compute_reqs']}"
    # SF-4: EXACTLY one gain.wasm fetch (the instantiate-once dedup) -- `>= 1` could not catch a
    # double-fetch, which the spec forbids.
    assert len(driven["wasm_reqs"]) == 1, f"expected exactly one gain.wasm fetch (dedup): {driven['wasm_reqs']}"
    # server-side counter delta 0 during the in-tab compute
    assert str(driven["count_before"]) in ("0", "0.0"), driven["count_before"]


def test_marker_paired_control_server_round_trip_moves_counter(driven):
    # PAIRED NEGATIVE CONTROL (B4): the server-round-trip control MUST move the counter, so the
    # in-tab `delta==0` is not vacuous.
    assert float(driven["count_after"]) > float(driven["count_before"]), (
        f"server round-trip did not move the counter ({driven['count_before']} -> {driven['count_after']}); "
        "the python_calls marker is vacuous"
    )


# ---- Island-absent paired control (S9): a scalar payload renders no island --------------------

def test_island_absent_control_scalar_payload_has_no_reactflow(driven):
    """PAIRED island-absent control (S9): the SAME page carries a scalar (kind-less) `WasmFunction`
    (`#scalar-wf`); it dispatches to the NON-island branch of Index.svelte and mounts NO
    `.react-flow__node`, while `#flowgraph` mounts one -- proving the island-mount assertion detects
    absence (the existing scalar path is the RED case, not a build/env bypass flag)."""
    from playwright.sync_api import sync_playwright

    with _App() as app:
        with sync_playwright() as p:
            b = p.chromium.launch(headless=True)
            page = b.new_context().new_page()
            page.goto(app.url, wait_until="load")
            page.wait_for_selector(".react-flow__node", timeout=60000)
            page.wait_for_selector("#scalar-wf", timeout=60000)
            counts = page.evaluate(r"""() => ({
                flow_nodes: document.querySelectorAll('.react-flow__node').length,
                scalar_nodes: document.querySelectorAll('#scalar-wf .react-flow__node').length,
                scalar_present: !!document.querySelector('#scalar-wf'),
            })""")
            b.close()
    assert counts["scalar_present"], "the scalar WasmFunction control component is missing"
    assert counts["flow_nodes"] > 0, "the flowgraph island did not mount (positive side of the control)"
    assert counts["scalar_nodes"] == 0, f"the scalar (island-absent) component mounted an island: {counts}"


# ---- B-2 (SECURITY, live): a client-supplied `wasm_path` never becomes a served directory --------

_ECHO_APP = r'''
import sys
sys.path.insert(0, DEMO_DIR)
import gradio as gr
from pythscribe.gradio import FlowGraph, client_callback
from kernels import gain

with gr.Blocks() as demo:
    spec = client_callback(gain, shape="node")
    fg = FlowGraph(
        nodes=[{"id": "x", "type": "source", "value": 2.0, "dtype": "float", "label": "x"},
               {"id": "k", "type": "source", "value": 3.0, "dtype": "float", "label": "k"},
               {"id": "g", "type": "compute", "kernel": spec, "label": "gain(x, k)"}],
        edges=[("x", "g"), ("k", "g")], elem_id="flowgraph")
    # the reviewer's PoC wiring: an ordinary echo handler -> postprocess runs on CLIENT data
    fg.change(lambda v: v, [fg], [fg], api_name="echo")

if __name__ == "__main__":
    demo.launch()
'''


def _http(method: str, url: str, body: dict | None = None, timeout: float = 30.0) -> tuple[int, str]:
    import json
    import urllib.error

    data = json.dumps(body).encode() if body is not None else None
    req = urllib.request.Request(url, data=data, method=method, headers={"Content-Type": "application/json"} if data else {})
    try:
        with urllib.request.urlopen(req, timeout=timeout) as r:
            return r.status, r.read().decode("utf-8", "replace")
    except urllib.error.HTTPError as e:
        return e.code, e.read().decode("utf-8", "replace")


def test_b2_live_client_wasm_path_is_not_served(tmp_path):
    """B-2 LIVE (the reviewer's PoC, end-to-end against a real Gradio server): a crafted value with
    `kernel.wasm_path=<victim dir>/x.wasm` POSTed to the echo endpoint must NOT make
    `GET /gradio_api/file=<victim dir>/id_rsa` readable (403 before AND after; the echoed kernel comes
    back `wasm: null` with no `wasm_path`), while the genuine server-built node's artifact URL (from
    the app config) IS served (200, a real `.wasm` -- the positive twin on the same server)."""
    import json

    victim = tmp_path / "victim_home"
    victim.mkdir()
    (victim / "id_rsa").write_text("-----BEGIN PRIVATE KEY----- victim secret", encoding="utf-8")
    (victim / "whatever.wasm").write_bytes(b"\0asm\1\0\0\0")
    app_py = tmp_path / "echo_app.py"
    app_py.write_text(_ECHO_APP.replace("DEMO_DIR", repr(str(DEMO))), encoding="utf-8")
    app = _App()
    app.proc = None
    app.log = tmp_path / "_echo_app.log"
    app._f = open(app.log, "w", encoding="utf-8")
    app.proc = subprocess.Popen([sys.executable, str(app_py)], cwd=str(tmp_path), env=_app_env(app.port),
                                stdout=app._f, stderr=subprocess.STDOUT)
    try:
        deadline = time.time() + 120
        while True:
            if app.proc.poll() is not None:
                raise RuntimeError(f"echo app exited early:\n{app.log.read_text(encoding='utf-8')[-3000:]}")
            try:
                if _http("GET", app.url + "/config")[0] == 200:
                    break
            except Exception:
                pass
            assert time.time() < deadline, "echo app did not come up"
            time.sleep(0.5)
        secret_url = app.url + "/gradio_api/file=" + (victim / "id_rsa").resolve().as_posix()
        assert _http("GET", secret_url)[0] in (403, 404), "the victim file must not be readable before the PoC"

        # positive twin: the genuine server-built artifact URL from the config IS served
        cfg = json.loads(_http("GET", app.url + "/config")[1])
        fg_cfg = next(c for c in cfg["components"] if c.get("type") == "flowgraph")  # Gradio 6: elem_id is not in props
        value = fg_cfg["props"]["value"]
        kern = next(n["kernel"] for n in value["nodes"] if n["type"] == "compute")
        assert "wasm_path" not in kern and isinstance(kern["wasm"], str) and kern["wasm"].startswith("/gradio_api/file="), kern
        assert "wasm_path" not in json.dumps(value)
        st, body = _http("GET", app.url + kern["wasm"])
        assert st == 200 and body.startswith("\0asm"), f"genuine artifact not served: {st} {body[:40]!r}"

        # the PoC: echo a crafted value naming the victim directory
        crafted = {**value, "nodes": [
            {**n, "kernel": {**n["kernel"], "wasm": None, "wasm_path": str(victim / "whatever.wasm")}} if n["type"] == "compute" else n
            for n in value["nodes"]]}
        st, body = _http("POST", app.url + "/gradio_api/call/echo", {"data": [crafted]})
        assert st == 200, (st, body)
        event_id = json.loads(body)["event_id"]
        st, stream = _http("GET", app.url + f"/gradio_api/call/echo/{event_id}", timeout=60)
        assert st == 200 and "event: complete" in stream, stream[-800:]
        echoed = json.loads([ln for ln in stream.splitlines() if ln.startswith("data:")][-1][5:].strip())[0]
        ek = next(n["kernel"] for n in echoed["nodes"] if n["type"] == "compute")
        assert ek["wasm"] is None and "wasm_path" not in ek, f"the echoed client path was given a transport: {ek}"

        # the whole point: the victim directory is STILL not served after the echo
        st, body = _http("GET", secret_url)
        assert st in (403, 404) and "victim secret" not in body, f"B-2 REOPENED: victim file served ({st}): {body[:80]!r}"
        st, _ = _http("GET", app.url + "/gradio_api/file=" + (victim / "whatever.wasm").resolve().as_posix())
        assert st in (403, 404), "an attacker-named existing file must not be served either"
    finally:
        app.__exit__(None, None, None)


# ---- SF-4 / SF-6 : load-BLOCKED and load-DELAYED controls (spec V3, previously absent) ----------

def test_sf4_load_blocked_shows_error_state_no_stale_number():
    """SF-4/SF-6: with `gain.wasm` fetch BLOCKED (`route.abort`), the node shows an explicit ERROR
    state -- the marker path becomes `error` (not a stuck `loading`), the value renders "error"
    (NEVER a stale number or `NaN`), and the page does not throw. The wasm was requested exactly once
    (dedup)."""
    from playwright.sync_api import sync_playwright

    with _App() as app:
        with sync_playwright() as p:
            b = p.chromium.launch(headless=True)
            ctx = b.new_context()
            ctx.add_init_script(_MARKER_RECORDER)
            page = ctx.new_page()
            page_errors: list[str] = []   # uncaught JS exceptions (must be none -- graceful)
            console_errors: list[str] = []  # console.error (a blocked fetch legitimately logs one)
            reqs: list[str] = []
            page.on("pageerror", lambda e: page_errors.append(f"pageerror: {e}"))
            page.on("console", lambda m: console_errors.append(m.text) if m.type == "error" else None)
            page.on("request", lambda r: reqs.append(r.url))
            page.route("**/*gain.wasm*", lambda route: route.abort())
            page.goto(app.url, wait_until="load")
            page.wait_for_selector(".react-flow__node", timeout=60000)
            page.wait_for_function("() => window.__pythscribeFlow && window.__pythscribeFlow.path === 'error'", timeout=60000)
            info = page.evaluate(r"""() => {
                const val = document.querySelector('[data-testid=flow-node-value]');
                return {
                    value: val ? val.textContent : null,
                    marker: window.__pythscribeFlow,
                    paths: window.__flowMarkerPaths,
                    node_error: !!document.querySelector('[data-testid=flow-node-error]'),
                };
            }""")
            b.close()
    assert info["marker"]["path"] == "error", info                          # SF-6 explicit error state
    assert "error" in (info["paths"] or []), f"no error marker recorded: {info}"
    assert info["value"] == "error", f"blocked load rendered a stale number/NaN: {info}"  # no stale number
    assert info["node_error"], "the node must surface an explicit error element on a blocked fetch"
    # graceful: no UNCAUGHT exception, and no JS logic error (TypeError/NaN) -- a failed-resource
    # console error from the aborted fetch is expected and allowed.
    assert not page_errors, f"a blocked fetch must be graceful (no pageerror): {page_errors}"
    assert not any(("TypeError" in e or "NaN" in e) for e in console_errors), f"logic error on block: {console_errors}"
    wasm_reqs = [u for u in reqs if "gain.wasm" in u]
    assert len(wasm_reqs) == 1, f"expected exactly one (aborted) gain.wasm fetch: {wasm_reqs}"


def test_sf4_load_delayed_shows_loading_then_value_never_typeerror():
    """SF-4: with `gain.wasm` fetch DELAYED, the async-instantiate-vs-sync-call seam holds -- the
    marker passes through `loading` and only THEN computes (never a `TypeError`/`NaN` from calling a
    kernel before its module exists), settling on the CPython value."""
    import time as _t

    from playwright.sync_api import sync_playwright

    oracle = _cpython_oracle_gain()
    with _App() as app:
        with sync_playwright() as p:
            b = p.chromium.launch(headless=True)
            ctx = b.new_context()
            ctx.add_init_script(_MARKER_RECORDER)
            page = ctx.new_page()
            errors: list[str] = []
            page.on("pageerror", lambda e: errors.append(f"pageerror: {e}"))
            page.on("console", lambda m: errors.append(f"{m.type}: {m.text}") if m.type == "error" else None)
            page.route("**/*gain.wasm*", lambda route: (_t.sleep(2.5), route.continue_()))
            page.goto(app.url, wait_until="load")
            page.wait_for_function(
                "() => { const e = document.querySelector('[data-testid=flow-node-value]'); return e && e.textContent && e.textContent !== 'loading'; }",
                timeout=60000)
            info = page.evaluate(r"""() => {
                const val = document.querySelector('[data-testid=flow-node-value]');
                return { value: val ? val.textContent : null, marker: window.__pythscribeFlow, paths: window.__flowMarkerPaths };
            }""")
            b.close()
    assert "loading" in (info["paths"] or []), f"delayed load never showed a loading marker: {info}"
    assert info["marker"]["path"] == "browser-wasm", f"delayed load did not settle browser-wasm: {info}"
    assert float(info["value"]) == oracle, f"delayed in-tab value {info['value']!r} != oracle {oracle!r}"
    assert not any(("TypeError" in e or "NaN" in e) for e in errors), f"the async->sync seam threw: {errors}"
    assert not errors, f"delayed load must be graceful: {errors}"
