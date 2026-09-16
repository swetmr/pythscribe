"""The headless (Playwright/Chromium) driver for `browser_scalar_client_app.py` -- ONE source of
truth used by BOTH `tests/pythscribe/test_browser_scalar_client_e2e.py` (the asserting gate) and
`browser_scalar_client.ipynb` (the reporting cell, which runs it as a subprocess).

It launches the app as a subprocess, drives the IN-TAB tab (slider moves, typed card numbers and
texts, loan sliders) and the SERVER tab (the paired negative control), and returns, per path:
  * `compute_requests`  -- requests to /gradio_api/* OTHER than static `file=` fetches during the
                           interaction (the client-side claim: 0 on the in-tab path)
  * `counters_delta`    -- the SERVER-side kernel counters (python_calls + server_calls) since the
                           interaction began, per kernel, from the app's /pythscribe-probe (0 in-tab)
  * `median_ms`         -- input set -> the status box shows the new call, median over the filter moves
  * per interaction: the float the tab computed (`value` + its IEEE-754 `bits`), the args the
    transforms produced, the kernel ms, and the DISPLAYED result text -- for the bit-for-bit
    comparison against the CPython kernel run on independent Python twins of the transforms
    (`references()` below), which the test and the notebook both run.

Run as a script (a SUBPROCESS: Windows' Proactor loop lets Playwright spawn Chromium; Jupyter's
Selector loop cannot) it prints one `RESULT_JSON:{...}` line.
"""
from __future__ import annotations

import json
import os
import random
import socket
import struct
import subprocess
import sys
import tempfile
import time
import urllib.request
from pathlib import Path
from typing import Any

HERE = Path(__file__).resolve().parent
APP = HERE / "browser_scalar_client_app.py"

THRESHOLDS = (0.13, 0.42, 0.77, 0.05, 0.9, 0.5, 0.33, 0.61, 0.25, 0.88)
CARDS = ("4242 4242 4242 4242", "4242-4242-4242-4241", "5555 5555 5555 4444", "378282246310005", "1234 5678 9012 3456", "")
TEXTS = ("card 4242424242424242 ssn 123456789", "no digits here", "café 9876543210 naïve ½", "12345678 short 123456789012 long")
MONTHS = (360, 180, 120, 60, 12)
RATES = (5.0, 0.0, 7.25)  # 0.0 exercises the `r == 0.0` branch
PRINCIPAL = 250000.0
MIN_RUN = 9
KERNEL_KEYS = ("filter", "luhn", "pii", "loan")


# ---- references: the CPython kernel on INDEPENDENT Python twins of the Arg transforms ------------
def data() -> list[float]:
    rng = random.Random(7)  # the app's seed
    return [rng.random() for _ in range(2000)]


def py_digits(s: str) -> list[int]:
    return [int(c) for c in (s or "") if "0" <= c <= "9"]


def py_utf8(s: str) -> list[int]:
    return list((s or "").encode("utf-8"))


def float_bits(v: float) -> str:
    return struct.pack("<d", float(v)).hex()


def kernels():
    """The kernels module by PATH under a name the app does not use (an independent instance)."""
    import importlib.util

    spec = importlib.util.spec_from_file_location("wasm_use_case_kernels_probe", HERE / "kernels.py")
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod


def references(mod=None) -> dict[str, dict[str, Any]]:
    """key -> {args, ref (float), bits, shown (the expected displayed text)} for every interaction
    the driver performs, computed by the kernels' PYTHON bodies (`run_python`) on the twins."""
    from pythscribe import binding_of

    mod = mod or kernels()
    d = data()
    refs: dict[str, dict[str, Any]] = {}

    def add(key, fn, args, shown):
        v = binding_of(fn).run_python(*args)
        refs[key] = {"kernel": fn.__name__, "args": args, "ref": v, "bits": float_bits(v), "shown": shown(v)}

    for t in THRESHOLDS:
        add(f"filter@{t}", mod.count_above, [d, float(t)], lambda v, t=t: f"{v:g} of {len(d)} above {t:.2f}")
    for c in CARDS:
        dg = py_digits(c)
        add(f"luhn@{c}", mod.luhn_ok, [dg], lambda v, dg=dg: "enter a card number" if not dg else ("VALID card (Luhn)" if v == 0 else "INVALID (fails Luhn)"))
    for s in TEXTS:
        add(f"pii@{s}", mod.pii_scan, [py_utf8(s), MIN_RUN], lambda v: f"{v:g} digit run(s) of >= {MIN_RUN} -- redact before upload" if v else "no long digit runs")
    for m in MONTHS:
        add(f"loan@m{m}", mod.monthly_payment, [PRINCIPAL, RATES[0], m], lambda v: f"${v:,.2f} / month")
    for r in RATES[1:]:
        add(f"loan@r{r:g}", mod.monthly_payment, [PRINCIPAL, r, MONTHS[-1]], lambda v: f"${v:,.2f} / month")
    return refs


def perturbed_references(mod=None) -> dict[str, str]:
    """The PAIRED CONTROL for the fidelity claim: the same kernels on a NEIGHBOURING input (a
    threshold shifted, a card digit flipped, one digit dropped from a run, months+1). Their bits
    must NOT equal the in-tab bits -- the equality is discriminating, not vacuous."""
    from pythscribe import binding_of

    mod = mod or kernels()
    d = data()
    out = {}
    t = THRESHOLDS[0]
    out[f"filter@{t}"] = float_bits(binding_of(mod.count_above).run_python(d, t + 0.05))
    c = CARDS[0]
    dg = py_digits(c)
    dg[-1] = (dg[-1] + 1) % 10
    out[f"luhn@{c}"] = float_bits(binding_of(mod.luhn_ok).run_python(dg))
    s = TEXTS[0]
    out[f"pii@{s}"] = float_bits(binding_of(mod.pii_scan).run_python(py_utf8(s.replace("123456789", "12345678")), MIN_RUN))
    m = MONTHS[0]
    out[f"loan@m{m}"] = float_bits(binding_of(mod.monthly_payment).run_python(PRINCIPAL, RATES[0], m + 1))
    return out


# ---- the app subprocess ---------------------------------------------------------------------------
def free_port() -> int:
    with socket.socket() as s:
        s.bind(("127.0.0.1", 0))
        return s.getsockname()[1]


class App:
    def __init__(self, timeout: float = 120.0):
        self.port = free_port()
        self.url = f"http://127.0.0.1:{self.port}"
        self.timeout = timeout
        self.log = Path(tempfile.gettempdir()) / f"pythscribe_browser_scalar_app_{self.port}.log"

    def __enter__(self) -> "App":
        env = {**os.environ, "GRADIO_SERVER_PORT": str(self.port), "GRADIO_ANALYTICS_ENABLED": "False", "PYTHONUTF8": "1"}
        self._logf = open(self.log, "w", encoding="utf-8")
        self.proc = subprocess.Popen([sys.executable, str(APP)], cwd=str(HERE), env=env, stdout=self._logf, stderr=subprocess.STDOUT)
        deadline = time.time() + self.timeout
        while time.time() < deadline:
            if self.proc.poll() is not None:
                self._logf.close()
                raise RuntimeError(f"app exited early ({self.proc.returncode}):\n{self.log.read_text(encoding='utf-8')[-3000:]}")
            try:
                self.probe()
                return self
            except Exception:
                time.sleep(0.5)
        self.__exit__(None, None, None)
        raise TimeoutError(f"app did not come up on {self.url}:\n{self.log.read_text(encoding='utf-8')[-3000:]}")

    def __exit__(self, *exc) -> None:
        if self.proc.poll() is None:
            self.proc.kill()
            try:
                self.proc.wait(timeout=15)
            except subprocess.TimeoutExpired:
                pass
        self._logf.close()

    def probe(self) -> dict[str, Any]:
        with urllib.request.urlopen(self.url + "/pythscribe-probe", timeout=5) as r:
            return json.loads(r.read())


def _counter_deltas(before: dict[str, Any], after: dict[str, Any]) -> dict[str, int]:
    """PER-KERNEL (python_calls + server_calls) delta -- asserted per kernel, so a marker that
    cannot move for one kernel is not hidden inside a cross-kernel sum."""
    return {k: (int(after[k]["python_calls"]) + int(after[k]["server_calls"]))
            - (int(before[k]["python_calls"]) + int(before[k]["server_calls"])) for k in after}


# JS helpers shared by both tabs: set a slider / textbox / number, click a tab, wait for a status marker.
_JS_LIB = """
const sleep = ms => new Promise(r => setTimeout(r, ms));
const text = sel => { const el = document.querySelector(sel + ' textarea, ' + sel + ' input'); return el ? el.value : ''; };
const setValue = (sel, v) => {
  // the RANGE input first: a gr.Slider renders its number box BEFORE the range, so a union selector would pick the wrong one
  const el = document.querySelector(sel + ' input[type=range]') || document.querySelector(sel + ' textarea, ' + sel + ' input[type=text], ' + sel + ' input[type=number]');
  if (!el) throw new Error('no input for ' + sel);
  const proto = el.tagName === 'TEXTAREA' ? window.HTMLTextAreaElement.prototype : window.HTMLInputElement.prototype;
  Object.getOwnPropertyDescriptor(proto, 'value').set.call(el, String(v));
  el.dispatchEvent(new Event('input', {bubbles: true})); el.dispatchEvent(new Event('change', {bubbles: true}));
};
const clickTab = t => { const b = [...document.querySelectorAll('button[role=tab]')].find(x => x.textContent.trim().startsWith(t)); b.click(); };
const waitFor = async (pred, limit) => { let w = 0; while (w < limit) { if (pred()) return true; await sleep(1); w += 1; } return pred(); };
// set an input, then wait until the STATUS box shows `marker` (a per-call sequence number -> monotonic, never a repeat)
const timed = async (inSel, stSel, v, marker, limit) => { const t0 = performance.now(); setValue(inSel, v);
  const ok = await waitFor(() => text(stSel).includes(marker), limit); return { ms: performance.now() - t0, ok }; };
"""

_JS_CLIENT = _JS_LIB + """
const [thresholds, cards, texts, months, rates] = A;
const C = () => window.pythscribeScalar;
const recs = {};
const take = (key, outSel, ms) => { const l = C().last; recs[key] = { value: l.value, bits: l.bits, args: l.args, kernel_ms: l.ms, shown: text(outSel), ms }; };
const move = async (key, inSel, outSel, stSel, v, limit) => { const n = C().calls + 1; const m = await timed(inSel, stSel, v, '(call ' + n + ')', limit);
  if (!m.ok) throw new Error('no in-tab result for ' + key + ' (calls=' + C().calls + ', wanted ' + n + '): ' + text(stSel) + ' / ' + (C().lastError || ''));
  if (C().calls !== n) throw new Error('call count moved by ' + (C().calls - n + 1) + ' for ' + key); take(key, outSel, m.ms); return m; };
// warm-up (the first call instantiates the .wasm -- a one-time cost, excluded from the median).
// Every value set differs from the component's CURRENT value (a same-value set fires no change event).
await move('warm@filter', '#c-thr', '#c-thr-out', '#c-thr-status', 0.55, 15000);
await move('warm@filter2', '#c-thr', '#c-thr-out', '#c-thr-status', 0.6, 15000);
const ts = [];
for (const t of thresholds) { const m = await move('filter@' + t, '#c-thr', '#c-thr-out', '#c-thr-status', t, 15000); ts.push(m.ms); }
await move('warm@luhn', '#c-card', '#c-card-out', '#c-card-status', '4111 1111 1111 1111', 15000);
for (const c of cards) await move('luhn@' + c, '#c-card', '#c-card-out', '#c-card-status', c, 15000);
await move('warm@pii', '#c-text', '#c-text-out', '#c-text-status', 'warm 000000000', 15000);
for (const s of texts) await move('pii@' + s, '#c-text', '#c-text-out', '#c-text-status', s, 15000);
await move('warm@loan', '#c-m', '#c-loan-out', '#c-loan-status', 300, 15000);
for (const m of months) await move('loan@m' + m, '#c-m', '#c-loan-out', '#c-loan-status', m, 15000);
for (const r of rates.slice(1)) await move('loan@r' + r, '#c-r', '#c-loan-out', '#c-loan-status', r, 15000);
ts.sort((a, b) => a - b);
const kms = Object.keys(recs).filter(k => k.startsWith('filter@')).map(k => recs[k].kernel_ms).sort((a, b) => a - b);
return { median_ms: ts[ts.length >> 1], kernel_median_ms: kms[kms.length >> 1], calls: C().calls, load_error: C().loadError, last_error: C().lastError,
         layout: C().layout, status: text('#c-loan-status'), recs };
"""

_JS_SERVER = _JS_LIB + """
const [thresholds, card, txt, months] = A;
clickTab('Server');
await sleep(300);
// the server status carries a per-kernel `call #n`: wait for the NEXT number (monotonic, never a repeat)
const callNo = stSel => { const m = /call #(\\d+)/.exec(text(stSel)); return m ? parseInt(m[1]) : 0; };
const smove = async (inSel, stSel, v, limit) => timed(inSel, stSel, v, 'call #' + (callNo(stSel) + 1) + ';', limit);
await smove('#s-thr', '#s-thr-status', 0.55, 30000); await smove('#s-thr', '#s-thr-status', 0.6, 30000);
const ts = [];
for (const t of thresholds) { const m = await smove('#s-thr', '#s-thr-status', t, 30000); if (!m.ok) throw new Error('no server result for thr ' + t + ': ' + text('#s-thr-status')); ts.push(m.ms); }
ts.sort((a, b) => a - b);
const thrStatus = text('#s-thr-status');
// the control must exercise EVERY kernel (the counter marker is asserted per kernel)
if (!(await smove('#s-card', '#s-card-status', card, 30000)).ok) throw new Error('no server luhn result: ' + text('#s-card-status'));
if (!(await smove('#s-text', '#s-text-status', txt, 30000)).ok) throw new Error('no server pii result: ' + text('#s-text-status'));
if (!(await smove('#s-m', '#s-loan-status', months, 30000)).ok) throw new Error('no server loan result: ' + text('#s-loan-status'));
return { median_ms: ts[ts.length >> 1], status: thrStatus, last_status: text('#s-loan-status') };
"""


def _is_compute(url: str) -> bool:
    """A server request that is NOT a static file fetch: /gradio_api/queue/join, /queue/data,
    /upload, /run/predict, /heartbeat ... (and any WebSocket URL, which never carries file=)."""
    return ("/gradio_api/" in url and "/gradio_api/file=" not in url) or url.startswith(("ws://", "wss://"))


def drive(app: App, *, server_control: bool = True) -> dict[str, Any]:
    from playwright.sync_api import sync_playwright

    reqs: list[str] = []
    errors: list[str] = []
    with sync_playwright() as p:
        br = p.chromium.launch()
        try:
            pg = br.new_page()
            pg.on("request", lambda r: reqs.append(r.url))
            pg.on("websocket", lambda ws: reqs.append(ws.url))  # transport-independent marker (a WS handshake is not a "request")
            pg.on("pageerror", lambda e: errors.append(str(e)))
            pg.goto(app.url, wait_until="networkidle", timeout=60000)
            try:
                pg.wait_for_function("window.pythscribeScalar && window.pythscribeScalar.ready === true", timeout=30000)
            except Exception as e:  # surface the client's own diagnostic, not a bare timeout
                load_error = pg.evaluate("window.pythscribeScalar ? window.pythscribeScalar.loadError : 'no client object'")
                raise RuntimeError(f"in-tab client never became ready; loadError={load_error!r}; page errors={errors}") from e
            # ---- the in-tab path ----
            c_before = app.probe()
            n0 = len(reqs)
            client = pg.evaluate(f"async (a) => {{ const A = a;{_JS_CLIENT} }}",
                                 [list(THRESHOLDS), list(CARDS), list(TEXTS), list(MONTHS), list(RATES)])
            pg.wait_for_timeout(300)
            c_reqs = reqs[n0:]
            c_after = app.probe()
            client["compute_requests"] = [u for u in c_reqs if _is_compute(u)]
            client["file_requests"] = [u for u in c_reqs if "/gradio_api/file=" in u]
            client["wasm_requests"] = [u for u in c_reqs if u.endswith(".wasm")]
            client["counters_delta_per_kernel"] = _counter_deltas(c_before, c_after)
            client["counters_delta"] = sum(client["counters_delta_per_kernel"].values())
            client["counters_after"] = c_after
            result: dict[str, Any] = {"client": client, "page_errors": list(errors)}
            # ---- the server path (paired negative control) ----
            if server_control:
                s_before = app.probe()
                n1 = len(reqs)
                # (each value differs from the component's initial value -- a same-value set fires no change event)
                server = pg.evaluate(f"async (a) => {{ const A = a;{_JS_SERVER} }}", [list(THRESHOLDS), CARDS[1], TEXTS[1], MONTHS[1]])
                pg.wait_for_timeout(300)
                s_reqs = reqs[n1:]
                s_after = app.probe()
                server["compute_requests"] = [u for u in s_reqs if _is_compute(u)]
                server["counters_delta_per_kernel"] = _counter_deltas(s_before, s_after)
                server["counters_delta"] = sum(server["counters_delta_per_kernel"].values())
                result["server"] = server
        finally:
            br.close()
    return result


def fidelity(result: dict[str, Any], mod=None) -> dict[str, dict[str, Any]]:
    """Per interaction: whether the in-tab float's IEEE-754 bits AND the displayed text equal the
    CPython reference (the kernel's Python body on independent transform twins)."""
    refs = references(mod)
    recs = result["client"]["recs"]
    out: dict[str, dict[str, Any]] = {}
    for key, ref in refs.items():
        got = recs.get(key)
        out[key] = {
            "kernel": ref["kernel"],
            "ref": ref["ref"],
            "value": None if got is None else got["value"],
            "bits_equal": got is not None and got["bits"] == ref["bits"],
            "shown": None if got is None else got["shown"],
            "shown_equal": got is not None and got["shown"] == ref["shown"],
            "args_equal": got is not None and _args_equal(got["args"], ref["args"]),
            "kernel_ms": None if got is None else got["kernel_ms"],
        }
    return out


def _args_equal(a: list, b: list) -> bool:
    if len(a) != len(b):
        return False
    for x, y in zip(a, b):
        if isinstance(y, list):
            if not (isinstance(x, list) and len(x) == len(y) and all(float(p) == float(q) for p, q in zip(x, y))):
                return False
        elif float(x) != float(y):
            return False
    return True


def main() -> None:
    with App() as app:
        res = drive(app)
    fid = fidelity(res)
    c, s = res["client"], res.get("server", {})
    summary = {
        "client": {k: v for k, v in c.items() if k not in ("recs", "counters_after")},
        "server": s,
        "fidelity": fid,
        "all_bit_for_bit": all(v["bits_equal"] and v["shown_equal"] and v["args_equal"] for v in fid.values()),
        "n_interactions": len(fid),
        "page_errors": res["page_errors"],
    }
    print("RESULT_JSON:" + json.dumps(summary))


if __name__ == "__main__":
    main()
