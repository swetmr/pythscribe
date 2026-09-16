"""Measure the TRUE browser-only interaction latency for browser_wasm_demos.ipynb: a slider drives
a `@wasm` kernel and updates the display ENTIRELY in the page — no Gradio, no server round-trip.
This is the latency a client-side component (v0.2.6 fix A) would deliver. Run as a SUBPROCESS
(fresh process → Windows ProactorEventLoop so Playwright can spawn Chromium). Prints RESULT_JSON.
"""
import functools
import http.server
import json
import random
import socketserver
import sys
import threading
import time
from pathlib import Path

from playwright.sync_api import sync_playwright

from pythscribe import binding_of, wasm


@wasm
def count_above(xs: list[float], thr: float) -> float:
    n = 0.0
    for x in xs:
        if x > thr:
            n = n + 1.0
    return n


def main() -> None:
    n = int(sys.argv[1]) if len(sys.argv) > 1 else 2000
    count_above([0.1, 0.9], 0.5)
    entry = binding_of(count_above).artifact.entry
    adir = entry.parent
    rng = random.Random(7)
    data = [rng.random() for _ in range(n)]

    html = (
        "<!doctype html><html><head><meta charset=utf-8></head><body>"
        "<input type=range id=thr min=0 max=1 step=0.01 value=0.5>"
        "<pre id=out></pre><script type=module>"
        f"import {{ count_above }} from './{entry.name}';"
        f"const DATA={json.dumps(data)};"
        "const thr=document.getElementById('thr'),out=document.getElementById('out');"
        "function render(){out.textContent='above='+count_above(DATA,parseFloat(thr.value))+'@'+thr.value;}"
        "thr.addEventListener('input',render);window.__ready=true;render();"
        "</script></body></html>"
    )
    page = adir / "_clientonly.html"
    page.write_text(html, encoding="utf-8")

    httpd = socketserver.TCPServer(("127.0.0.1", 0), functools.partial(http.server.SimpleHTTPRequestHandler, directory=str(adir)))
    port = httpd.server_address[1]
    threading.Thread(target=httpd.serve_forever, daemon=True).start()
    try:
        with sync_playwright() as p:
            br = p.chromium.launch(); pg = br.new_page()
            pg.goto(f"http://127.0.0.1:{port}/_clientonly.html")
            pg.wait_for_function("window.__ready === true", timeout=15000)
            res = pg.evaluate("""async () => {
              const sleep = ms => new Promise(r=>setTimeout(r,ms));
              const thr = document.getElementById('thr'); const out = () => document.getElementById('out').textContent;
              const setV = v => { const s=Object.getOwnPropertyDescriptor(window.HTMLInputElement.prototype,'value').set; s.call(thr,v); thr.dispatchEvent(new Event('input',{bubbles:true})); };
              const vals=[0.15,0.3,0.45,0.6,0.75,0.25,0.55,0.7,0.35,0.65,0.2,0.8];
              setV(vals[0]); setV(vals[1]);
              const ts=[];
              for (let k=2;k<vals.length;k++){ const b=out(); const t0=performance.now(); setV(vals[k]);
                let w=0; while(w<2000){ if(out()!==b) break; await sleep(1); w+=1; } ts.push(performance.now()-t0); }
              ts.sort((a,b)=>a-b); return { median_ms: ts[ts.length>>1], last: out() };
            }""")
            br.close()
    finally:
        httpd.shutdown()
        page.unlink(missing_ok=True)
    res["n_points"] = n
    print("RESULT_JSON:" + json.dumps(res))


if __name__ == "__main__":
    main()
