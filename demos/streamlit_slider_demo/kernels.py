"""Kernels for the pythscribe @wasm Streamlit demo (callback path).

`response` is a small numeric function compiled to @wasm; it runs INSIDE the component iframe (a
slider `oninput`), so a drag recomputes in the browser tab with ZERO Streamlit server reruns. It
evaluates a cubic MANY times so the browser compute is a MEASURABLE few milliseconds -- a single
cubic is a fraction of a microsecond, too fast to time (the timer would just read 0.000 ms).
`response_py` is the SAME math in plain Python; the demo runs it on the SERVER (the ordinary
Streamlit way), where every slider drag reruns the whole script -- visibly slower, once per drag.

    python -m pythscribe.build examples/streamlit-callback/kernels.py
"""
from pythscribe import wasm

# The iteration count is written as a LITERAL (10_000_000) in BOTH functions below, not shared via a
# named constant: a `@wasm` kernel is compiled in isolation and cannot reference a module-level global
# (it fails with a type-lowering gap), so `response` must inline the count and `response_py` matches it
# by hand -- keep the two in sync. It is large enough that the browser @wasm compute is a measurable
# few ms and the plain-Python server recompute is ~tens-to-hundreds of ms (a visible "Running..." spinner).


@wasm
def response(x: float) -> float:
    # f(x) = x**3 - 2*x + 3, computed the hard way: a bounded recurrence whose fixed point is x,
    # summed over a million iterations so the work (and the timer) is real. The value tracks the
    # cubic as you drag the slider.
    total = 0.0
    t = x
    for _ in range(10000000):
        t = t * 0.9999 + 0.0001 * x
        total = total + (t * t * t - 2.0 * t + 3.0)
    return total / 10000000.0


def response_py(x: float) -> float:
    # SAME math as `response`, in plain Python -- the ordinary server-side Streamlit recompute.
    total = 0.0
    t = x
    for _ in range(10000000):
        t = t * 0.9999 + 0.0001 * x
        total += t * t * t - 2.0 * t + 3.0
    return total / 10000000.0
