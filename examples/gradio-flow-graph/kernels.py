"""M0 flagship demo kernels -- Python compiled to WASM, used as a ReactFlow custom node's
SYNCHRONOUS compute callback in the browser (the class of thing server-located Python cannot do)."""
from pythscribe import wasm


@wasm
def gain(x: float, k: float) -> float:  # a "gain" node: multiply an input by a factor
    return x * k
