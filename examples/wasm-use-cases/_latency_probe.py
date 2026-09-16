"""Headless latency probe for browser_wasm_demos.ipynb — measures the interaction latency
(slider -> result) of the browser `@wasm` path (WasmFunction: dispatch + in-tab compute +
result_of) vs a plain server path, for the same kernel, plus `python_calls` for each. Run as a
SUBPROCESS (fresh process → Windows ProactorEventLoop, so Playwright can spawn the browser;
Jupyter's SelectorEventLoop cannot). Prints one RESULT_JSON:{...} line.

    python _latency_probe.py [n_points]
"""
import json
import random
import socket
import sys
import time

import gradio as gr
from playwright.sync_api import sync_playwright

from pythscribe import wasm
import json as _json
from pythscribe.gradio import WasmFunction, bundle_url, dispatch, result_of


@wasm
def count_above(xs: list[float], thr: float) -> float:
    n = 0.0
    for x in xs:
        if x > thr:
            n = n + 1.0
    return n


def main() -> None:
    n = int(sys.argv[1]) if len(sys.argv) > 1 else 2000
    rng = random.Random(7)
    data = [rng.random() for _ in range(n)]

    def srv(thr):
        return f"server: {int(sum(1 for x in data if x > thr))} above {thr:.2f}"

    def b_dispatch(thr):
        return dispatch(count_above, data, float(thr))

    def b_show(payload):
        r = result_of(count_above, payload)
        if r is None or r.get("value") is None:
            return gr.skip()
        return f"browser: {int(r['value'])} (python_calls={r.get('python_calls')})"

    count_above([0.1, 0.9], 0.5)   # compile so bundle_url resolves
    burl = bundle_url(count_above)
    load_js = ("async () => { const m = await import('" + burl + "');"
               " window.__D = " + _json.dumps(data) + ";"
               " window.__k = (v) => 'client:' + m.count_above(window.__D, parseFloat(v)) + '@' + parseFloat(v).toFixed(2);"
               " window.__loaded = true; }")
    with gr.Blocks() as demo:
        gr.HTML("<style>.wasm-payload{display:none!important}</style>")
        thr_b = gr.Slider(0, 1, value=0.5, step=0.01, elem_id="thr-b")
        comp = WasmFunction()
        out_b = gr.Textbox(elem_id="out-b")
        thr_s = gr.Slider(0, 1, value=0.5, step=0.01, elem_id="thr-s")
        out_s = gr.Textbox(elem_id="out-s")
        thr_j = gr.Slider(0, 1, value=0.5, step=0.01, elem_id="thr-j")
        out_j = gr.Textbox(elem_id="out-j")
        demo.load(None, None, None, js=load_js)
        thr_b.change(b_dispatch, [thr_b], [comp]); comp.change(b_show, [comp], [out_b])
        thr_s.change(srv, [thr_s], [out_s])
        thr_j.change(None, [thr_j], [out_j], js="(v) => (window.__k ? window.__k(v) : 'loading')")

    port = 8500
    for cand in range(8500, 8600):
        try:
            with socket.socket() as s:
                s.bind(("127.0.0.1", cand)); port = cand; break
        except OSError:
            continue
    demo.launch(server_port=port, server_name="127.0.0.1", inbrowser=False, prevent_thread_lock=True)
    for _ in range(60):
        try:
            socket.create_connection(("127.0.0.1", port), timeout=1).close(); break
        except OSError:
            time.sleep(0.5)
    time.sleep(2)

    JS = """async () => {
      const sleep = ms => new Promise(r=>setTimeout(r,ms));
      const setS = (sel, v) => { const el = document.querySelector(sel+' input[type=range]');
        const set = Object.getOwnPropertyDescriptor(window.HTMLInputElement.prototype,'value').set; set.call(el, v);
        el.dispatchEvent(new Event('input',{bubbles:true})); el.dispatchEvent(new Event('change',{bubbles:true})); };
      const val = (sel) => (document.querySelector(sel+' textarea')||document.querySelector(sel+' input'))?.value||'';
      async function waitChange(outSel, before){ let w=0; while(w<9000){ const c=val(outSel); if(c && c!==before) return c; await sleep(4); w+=4;} return val(outSel); }
      async function bench(slider, outSel, values){
        let b=val(outSel); setS(slider, values[0]); b=await waitChange(outSel,b);
        setS(slider, values[1]); b=await waitChange(outSel,b);
        const ts=[];
        for(let k=2;k<values.length;k++){ const before=val(outSel); const t0=performance.now();
          setS(slider, values[k]); await waitChange(outSel, before); ts.push(performance.now()-t0); }
        ts.sort((a,b)=>a-b); return { median: ts[ts.length>>1], out: val(outSel) };
      }
      const vals=[0.15,0.3,0.45,0.6,0.75,0.25,0.55,0.7,0.35,0.65];
      const s = await bench('#thr-s', '#out-s', vals);
      const b = await bench('#thr-b', '#out-b', vals);
      const j = await bench('#thr-j', '#out-j', vals);
      return { server_ms: s.median, browser_ms: b.median, clientjs_ms: j.median, out_s: s.out, out_b: b.out, out_j: j.out };
    }"""
    try:
        with sync_playwright() as p:
            br = p.chromium.launch(); pg = br.new_page()
            pg.goto(f"http://127.0.0.1:{port}", wait_until="networkidle", timeout=30000)
            pg.wait_for_timeout(2500)
            res = pg.evaluate(JS)
            br.close()
    finally:
        demo.close()
    res["n_points"] = n
    print("RESULT_JSON:" + json.dumps(res))


if __name__ == "__main__":
    main()
