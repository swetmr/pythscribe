"""Pinned acceptance fixture for the self-contained pip wheel
(specs/13-09-26-lib-selfcontained-pip-wheel).

A single WASM-eligible scalar kernel: annotated params, scalar return -> the compiler
routes it to WASM (js+wasm), which is exactly what `pyths build examples/hello.py` and the
node-free CI acceptance job assert. Keep this trivial and STABLE: it is a hashed acceptance
witness, not a demo. As plain Python it is `2*x + 1`; the WASM/server path must return the
same value bit-for-bit.
"""
from pythscribe import wasm


@wasm
def hello(x: float) -> float:
    return x * 2.0 + 1.0
