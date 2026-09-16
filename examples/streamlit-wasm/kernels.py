"""The M0 demo kernel: one self-contained numeric preprocessing function.

`rms_gain` is the gain factor that scales a signal to a target RMS level -- the first step
of an audio/sensor normalisation pipeline. Its output stays in the browser (plan §4a): the
component shows the factor; nothing feeds back into the Python script.

Build the browser artifact (explicit, never on import):
    python -m pythscribe.build examples/streamlit-wasm/kernels.py
"""
from pythscribe import wasm


@wasm
def rms_gain(xs: list[float], target: float) -> float:
    acc = 0.0
    n = 0
    for x in xs:
        acc = acc + x * x
        n = n + 1
    if n == 0:
        return 1.0
    rms = (acc / n) ** 0.5
    if rms == 0.0:
        return 1.0
    return target / rms
