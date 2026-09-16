"""v0.2.6 fix A -- `pythscribe.gradio.client_side`: four SCALAR `@wasm` use cases run ENTIRELY in
the browser tab from Gradio's built-in `js=` hook (no custom component, no hand-written JS, no
server round-trip for the computation). The hand-written `js=` wiring of `browser_wasm_demos.ipynb`
collapses to `client_side(...)` + `Arg`s.

    python -m pythscribe.build examples/wasm-use-cases/kernels.py   # count_above, luhn_ok, pii_scan, monthly_payment
    python examples/wasm-use-cases/browser_scalar_client_app.py

Per event: the Gradio input value -> the `Arg` transform (in the tab) -> the kernel's own .wasm
through the pythscribe FFI shim (`ffi.call(..., {returnType: "float"})`) -> a float -> `format`
-> the output Textbox. The server does NOTHING for the computation (the E2E asserts 0 /gradio_api
compute requests AND a 0 delta on the kernels' server-side counters). NOT claimed: that the input
never reaches the server -- a Textbox / Slider value is Gradio component state (its initial value
comes from the server, and any other event listing the component as an input sends it); what
never leaves the tab is the COMPUTATION.

The "Server round-trip" tab is the PAIRED NEGATIVE CONTROL: the same kernels wired through Python
`fn`s -- driving it makes the same assertions go red (requests > 0, counters > 0 per kernel). The
`/pythscribe-probe` route reports the server-side counters for the E2E.
"""
from __future__ import annotations

import os
import random
import sys
import time
from pathlib import Path

import gradio as gr
from fastapi import FastAPI

from pythscribe import binding_of
from pythscribe.gradio import Arg, client_side

HERE = Path(__file__).resolve().parent


def _load_kernels(path: Path, modname: str):
    """Load `kernels.py` by PATH under a distinct module name (this dir and
    examples/gradio-image-preprocess both ship a `kernels.py`)."""
    import importlib.util

    spec = importlib.util.spec_from_file_location(modname, path)
    mod = importlib.util.module_from_spec(spec)
    sys.modules[modname] = mod
    spec.loader.exec_module(mod)
    return mod


_UC = _load_kernels(HERE / "kernels.py", "wasm_use_case_kernels_scalar")
count_above, luhn_ok, pii_scan, monthly_payment = _UC.count_above, _UC.luhn_ok, _UC.pii_scan, _UC.monthly_payment
KERNELS = {"filter": count_above, "luhn": luhn_ok, "pii": pii_scan, "loan": monthly_payment}

_rng = random.Random(7)  # seeded ONCE (a fresh RNG per element would be degenerate)
DATA = [_rng.random() for _ in range(2000)]  # the dataset the filter tab embeds in the handler (Arg.const)
MIN_RUN = 9  # digit runs of >= 9 count as card / SSN / phone-like

# the display formats (JS function expressions over `(value, info)`; the raw float is `value`)
FMT_FILTER = f"(v, i) => v + ' of {len(DATA)} above ' + i.args[1].toFixed(2)"
FMT_LUHN = "(v, i) => i.args[0].length === 0 ? 'enter a card number' : (v === 0 ? 'VALID card (Luhn)' : 'INVALID (fails Luhn)')"
FMT_PII = f"v => v ? v + ' digit run(s) of >= {MIN_RUN} -- redact before upload' : 'no long digit runs'"
FMT_LOAN = "v => '$' + v.toLocaleString('en-US', {minimumFractionDigits: 2, maximumFractionDigits: 2}) + ' / month'"


# ---- the SERVER path (the negative control): the same kernels run in-process by Python ----------
def py_digits(s: str) -> list[int]:
    return [int(c) for c in (s or "") if "0" <= c <= "9"]


def py_utf8(s: str) -> list[int]:
    return list((s or "").encode("utf-8"))


_server_calls = {k: 0 for k in KERNELS}


def _server(name: str, *args):
    fn = KERNELS[name]
    b = binding_of(fn)
    before = b.counts()
    t0 = time.perf_counter()
    value = fn(*args)
    ms = (time.perf_counter() - t0) * 1e3
    after = b.counts()
    _server_calls[name] += 1
    return value, (f"{name}: {ms:.2f} ms on the SERVER (mode={b.mode}); 1 round-trip; call #{_server_calls[name]}; "
                   f"python_calls+={after[0] - before[0]} server_calls+={after[1] - before[1]}")


def server_filter(thr: float):
    v, st = _server("filter", DATA, float(thr))
    return f"{v:g} of {len(DATA)} above {float(thr):.2f}", st


def server_luhn(card: str):
    d = py_digits(card)
    v, st = _server("luhn", d)
    return ("enter a card number" if not d else ("VALID card (Luhn)" if v == 0 else "INVALID (fails Luhn)")), st


def server_pii(text: str):
    v, st = _server("pii", py_utf8(text), MIN_RUN)
    return (f"{v:g} digit run(s) of >= {MIN_RUN} -- redact before upload" if v else "no long digit runs"), st


def server_loan(principal: float, rate: float, months: float):
    v, st = _server("loan", float(principal), float(rate), int(months))
    return f"${v:,.2f} / month", st


def build_demo() -> gr.Blocks:
    cs = client_side(KERNELS)  # validates every kernel's contract + resolves its artifact HERE (no fallback)
    with gr.Blocks(title="pythscribe @wasm - scalar kernels in the browser tab (client_side)") as demo:
        gr.Markdown("## `@wasm` scalar kernels **in your browser tab** — `pythscribe.gradio.client_side`, `js=` hook, no round-trip")
        demo.load(None, None, None, js=cs.loader_js)
        with gr.Tabs():
            with gr.Tab("In-tab (client-side)", id="client"):
                gr.Markdown("Every result below is computed **in the tab** by the kernel's own `.wasm` (via the pythscribe FFI shim); "
                            "the server does nothing per event. The inputs are ordinary Gradio component values.")
                with gr.Group():
                    gr.Markdown("**Client-side filter** — `count_above(DATA, thr)` over 2000 embedded values (`Arg.const(DATA), Arg.float`)")
                    c_thr = gr.Slider(0, 1, value=0.5, step=0.01, label="threshold", elem_id="c-thr")
                    c_thr_out = gr.Textbox(label="result", interactive=False, elem_id="c-thr-out")
                    c_thr_st = gr.Textbox(label="status", interactive=False, elem_id="c-thr-status")
                    c_thr.change(None, [c_thr], [c_thr_out, c_thr_st], js=cs.call_js([Arg.const(DATA), Arg.float], kernel="filter", format=FMT_FILTER, status=True))
                with gr.Group():
                    gr.Markdown("**Card validator** — `luhn_ok(digits)` (`Arg.digits`: the decimal digits of the text)")
                    c_card = gr.Textbox(value="4242 4242 4242 4242", label="card number", elem_id="c-card")
                    c_card_out = gr.Textbox(label="result", interactive=False, elem_id="c-card-out")
                    c_card_st = gr.Textbox(label="status", interactive=False, elem_id="c-card-status")
                    c_card.change(None, [c_card], [c_card_out, c_card_st], js=cs.call_js([Arg.digits], kernel="luhn", format=FMT_LUHN, status=True))
                with gr.Group():
                    gr.Markdown(f"**On-device PII scan** — `pii_scan(utf8_bytes, {MIN_RUN})` (`Arg.utf8_bytes, Arg.const({MIN_RUN})`)")
                    c_text = gr.Textbox(value="card 4242424242424242 ssn 123456789", label="text", lines=2, elem_id="c-text")
                    c_text_out = gr.Textbox(label="result", interactive=False, elem_id="c-text-out")
                    c_text_st = gr.Textbox(label="status", interactive=False, elem_id="c-text-status")
                    c_text.change(None, [c_text], [c_text_out, c_text_st], js=cs.call_js([Arg.utf8_bytes, Arg.const(MIN_RUN)], kernel="pii", format=FMT_PII, status=True))
                with gr.Group():
                    gr.Markdown("**Loan calculator** — `monthly_payment(principal, annual_rate_pct, months)`, wired the "
                                "**recommended named way**: each parameter is bound to its component BY NAME, so the "
                                "components cannot be positionally shifted (`call_js` returns `(js, inputs)` with `inputs` "
                                "already in signature order).")
                    with gr.Row():
                        c_p = gr.Number(value=250000, label="principal", elem_id="c-p")
                        c_r = gr.Slider(0, 15, value=5.0, step=0.05, label="annual rate %", elem_id="c-r")
                        c_m = gr.Slider(12, 360, value=360, step=1, label="months", elem_id="c-m")
                    c_loan_out = gr.Textbox(label="result", interactive=False, elem_id="c-loan-out")
                    c_loan_st = gr.Textbox(label="status", interactive=False, elem_id="c-loan-status")
                    loan_js, loan_inputs = cs.call_js(
                        {"principal": (Arg.float, c_p), "annual_rate_pct": (Arg.float, c_r), "months": (Arg.int, c_m)},
                        kernel="loan", format=FMT_LOAN, status=True)
                    for comp in (c_p, c_r, c_m):
                        comp.change(None, loan_inputs, [c_loan_out, c_loan_st], js=loan_js)
            with gr.Tab("Server round-trip (comparison / negative control)", id="server"):
                gr.Markdown("The SAME kernels wired through Python `fn`s: one round-trip per event, the server's counters advance. "
                            "(The E2E's negative control: this tab must FAIL the client-side assertions.)")
                with gr.Group():
                    s_thr = gr.Slider(0, 1, value=0.5, step=0.01, label="threshold", elem_id="s-thr")
                    s_thr_out = gr.Textbox(label="result", interactive=False, elem_id="s-thr-out")
                    s_thr_st = gr.Textbox(label="status", interactive=False, elem_id="s-thr-status")
                    s_thr.change(server_filter, [s_thr], [s_thr_out, s_thr_st])
                with gr.Group():
                    s_card = gr.Textbox(value="4242 4242 4242 4242", label="card number", elem_id="s-card")
                    s_card_out = gr.Textbox(label="result", interactive=False, elem_id="s-card-out")
                    s_card_st = gr.Textbox(label="status", interactive=False, elem_id="s-card-status")
                    s_card.change(server_luhn, [s_card], [s_card_out, s_card_st])
                with gr.Group():
                    s_text = gr.Textbox(value="card 4242424242424242 ssn 123456789", label="text", lines=2, elem_id="s-text")
                    s_text_out = gr.Textbox(label="result", interactive=False, elem_id="s-text-out")
                    s_text_st = gr.Textbox(label="status", interactive=False, elem_id="s-text-status")
                    s_text.change(server_pii, [s_text], [s_text_out, s_text_st])
                with gr.Group():
                    with gr.Row():
                        s_p = gr.Number(value=250000, label="principal", elem_id="s-p")
                        s_r = gr.Slider(0, 15, value=5.0, step=0.05, label="annual rate %", elem_id="s-r")
                        s_m = gr.Slider(12, 360, value=360, step=1, label="months", elem_id="s-m")
                    s_loan_out = gr.Textbox(label="result", interactive=False, elem_id="s-loan-out")
                    s_loan_st = gr.Textbox(label="status", interactive=False, elem_id="s-loan-status")
                    for comp in (s_p, s_r, s_m):
                        comp.change(server_loan, [s_p, s_r, s_m], [s_loan_out, s_loan_st])
    return demo


def build_app() -> FastAPI:
    api = FastAPI()

    @api.get("/pythscribe-probe")
    def probe():
        return {name: {"python_calls": binding_of(fn).counts()[0], "server_calls": binding_of(fn).counts()[1],
                       "mode": binding_of(fn).mode, "artifact_status": binding_of(fn).artifact_status}
                for name, fn in KERNELS.items()}

    return gr.mount_gradio_app(api, build_demo(), path="/")


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(
        build_app(),
        host=os.environ.get("GRADIO_SERVER_NAME", "127.0.0.1"),
        port=int(os.environ.get("GRADIO_SERVER_PORT", "7860")),
        log_level="warning",
    )
