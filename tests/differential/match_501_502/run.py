#!/usr/bin/env python3
"""match_501_502 — the match OR-alternative (#501) + param-capture (#502) 3-way differential.

Every `cases/*.ps` is a plain-Python PROGRAM whose printed output pins CPython's
match-statement binding semantics for:

    * #501  an OR-pattern whose alternatives bind the SAME name at DIFFERENT positions
            (`case [x, 1] | [1, x]`) — the value must come from the alternative that
            actually matched (old lowering: always the first alternative's slot);
    * #502  a capture whose name is a function/method PARAMETER rebinds the param
            (old lowering: a case-block `let` shadowed it; the post-match read saw the
            argument).

Each case runs on the same THREE arms as `../shadow_491` and is diffed as exact stdout:

    cpython   the pinned oracle (PYTHS_ORACLE_PYTHON, default `py -3.12`)
    js        `pyths compile --target js`      + node   (pyths-runtime -> runtime/src/index.js)
    js+wasm   `pyths compile --target js+wasm` + node   (glue entry: WASM + the JS twin)

`match` has NO WASM lowering (`crates/pyths_codegen_wasm` contains no Match arm), so
the js+wasm arm is the JS-demotion path for every function here — the point of running
it is to pin that `js == js+wasm == cpython` stays true through the glue entry.

The driver is the #491 harness (arms, rewiring, EXPECT/WASM/DEMOTE directives — see
`../shadow_491/shadow_491.py`) pointed at THIS directory's cases; the summary line it
prints is therefore tagged `[shadow_491]`.

Paired negative controls (verified by hand at the landing commit): reverting the
`Pattern::Or` arm of `emit_pattern_bindings_in` to bind from `alternatives.first()`
turns 01/02/03/04/05/06/10 RED (7 of 11); dropping `|| ctx.params.contains(&n)` from
`collect_hoisted_names`' Match arm turns 07/08/09/10 RED (4 of 11). Case 11 is the
OVER-FIX guard (never-reused capture keeps its block `let`; a guarded param capture
whose reads are all inside the case) — green under both mutants by design.

Run from the repo root (needs target/{release,debug}/pyths[.exe], node, the oracle):
    python tests/differential/match_501_502/run.py [--only NAME] [--keep]
Env: PYTHS_BIN, PYTHS_ORACLE_PYTHON (e.g. "py -3.12"), MATCH_501_502_SCRATCH.
"""
from __future__ import annotations

import importlib.util
import os
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
DRIVER = HERE.parent / "shadow_491" / "shadow_491.py"


def main() -> int:
    spec = importlib.util.spec_from_file_location("shadow_491_driver", DRIVER)
    assert spec is not None and spec.loader is not None, DRIVER
    driver = importlib.util.module_from_spec(spec)
    sys.modules[spec.name] = driver  # the driver's @dataclass bodies resolve via sys.modules
    spec.loader.exec_module(driver)
    # `driver.main()` reads these module globals at call time.
    driver.CASES = HERE / "cases"
    driver.SCRATCH = Path(os.environ.get("MATCH_501_502_SCRATCH") or (HERE / ".scratch"))
    return driver.main()


if __name__ == "__main__":
    sys.exit(main())
