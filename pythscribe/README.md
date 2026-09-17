# pythscribe — `@wasm` for ordinary Python

**One decorator. Your function stays Python. It runs compiled — in the browser tab, in-process on the
server (sandboxed, GIL-free, bit-for-bit), or as plain Python when nothing is built — and you can always
tell which.**

```bash
pip install pythscribe            # the decorator, the loader, the fallback, BOTH framework adapters
pip install pythscribe[server]    # + wasmtime: run kernels in-process on the server; ALSO the validator that lets
                                  #   the build adopt `wasm-opt` output -- optimized output is adopted only when a
                                  #   validator is installed (`pip install wasmtime` or `pythscribe[server]`); this
                                  #   applies to browser-tab artifacts too (see "Optimized artifacts")
pip install pythscribe[gradio]    # + Gradio (the component itself is already inside the wheel)
```

```python
from pythscribe import wasm

@wasm
def edit_distance(a: list[int], b: list[int], prev: list[int], cur: list[int]) -> int:
    n, m = len(a), len(b)
    for j in range(m + 1):
        prev[j] = j
    for i in range(1, n + 1):
        cur[0] = i
        for j in range(1, m + 1):
            cost = 0 if a[i - 1] == b[j - 1] else 1
            cur[j] = min(prev[j] + 1, cur[j - 1] + 1, prev[j - 1] + cost)
        for j in range(m + 1):
            prev[j] = cur[j]
    return prev[m]
```

```bash
python -m pythscribe.build kernels.py     # explicit, once: compiles each @wasm def with `pyths` -> __pythscribe__/<fn>/
```

That's the whole workflow. Without the build step the function is still your function (plain Python).
With it, the same call runs the compiled kernel — 10–20× faster than the interpreter on loops like the
one above — and the decorator tells you which path ran:

```python
>>> from pythscribe import binding_of
>>> b = binding_of(edit_distance)
>>> b.mode, b.mode_reason
('server', 'auto: artifact + wasmtime + FFI grammar')
>>> edit_distance([1, 2, 3], [1, 3], [0, 0, 0], [0, 0, 0])
1
>>> b.server_runs(), b.calls()      # in-process WASM runs, Python-body runs
(1, 0)
```

## Quickstart (Gradio app, browser + server)

```python
import gradio as gr
from pythscribe import wasm, binding_of
from pythscribe.gradio import WasmFunction, dispatch, result_of, describe

@wasm
def rms_gain(xs: list[float], target: float) -> float:
    acc = 0.0
    for x in xs:
        acc = acc + x * x
    rms = (acc / len(xs)) ** 0.5 if xs else 0.0
    return target / rms if rms else 1.0

with gr.Blocks() as demo:
    xs = gr.Textbox("1, 2, 3, 4"); target = gr.Number(0.5); btn = gr.Button("run")
    comp = WasmFunction(); out = gr.Textbox()
    btn.click(lambda s, t: (p := dispatch(rms_gain, [float(v) for v in s.split(",")], t), describe(result_of(rms_gain, p))), [xs, target], [comp, out])
    comp.change(lambda p: describe(result_of(rms_gain, p)), [comp], [out])
demo.launch()
```

`dispatch` sends the compiled bundle to the tab when an artifact is bound — the browser computes and the
result line says `path=browser-wasm python_calls=0 server_calls=0`. With no artifact (or if the tab
fails), the Python body runs on the server and the line says `path=python-fallback`. Nothing about the
path is taken on the client's word: the two counters are server-side.

## Streamlit (the callback path)

Streamlit reruns the whole script on every widget interaction, so keeping an `@wasm` recompute *off*
the server needs a custom in-iframe component, not a one-line hook. Two helpers wire it:

```python
from pythscribe import wasm
from pythscribe.streamlit import client_callback, slider_compute

@wasm
def response(x: float) -> float:                 # an arity-1 scalar kernel
    ...

spec = client_callback(response, shape="node")   # transport-neutral CallbackSpec (shared with Gradio)
slider_compute(spec, min=0.0, max=10.0, step=0.1, key="demo", label="x")
```

The slider's `oninput` runs the compiled kernel **synchronously in the tab** and re-renders inside the
iframe — it never calls `setComponentValue`, so a drag causes **zero server reruns** (`key` is required:
it keeps the same iframe across reruns). See `demos/streamlit_slider_demo/`.

**This is narrower, and less ergonomic, than `@wasm` in Gradio — by design of the host.** Gradio has a
first-class `js=` hook, so any client-side kernel wires up directly (`pythscribe.gradio.client_side`,
the `browser_*` adapters). Streamlit has no such hook, so the client path goes through a vendored custom
component, and today `slider_compute` hosts exactly **one slider driving an arity-1 scalar (`node`)
kernel**; any other shape or arity is refused up front with a clear Python error, never silently
degraded. `client_callback`/`CallbackSpec` are transport-neutral and reusable, but the Streamlit *host*
is not yet generic.

> **Note for future usage.** Broadening the Streamlit host — more widgets than a slider, multi-input
> and non-scalar kernels — is a future direction, not a current capability. Until then, on Streamlit:
> use `slider_compute` for the single-slider→scalar case, run the kernel on the server (the ordinary
> rerun) for anything else, or reach for the Gradio adapters where the `js=` hook makes the client path
> general.

## The three modes — one resolver, never a silent mis-selection

| `binding.mode` | What a direct Python call does | What the Gradio adapter does | When |
|---|---|---|---|
| `server` | runs the artifact's `.wasm` **in-process** under wasmtime (sandboxed) | still sends it to the tab | artifact built **and** `wasmtime` installed **and** the signature is in the FFI grammar |
| `browser` | runs the Python body | sends it to the tab | artifact built, server path unavailable here (e.g. no `wasmtime`) |
| `fallback` | runs the Python body | runs the Python body server-side | no usable artifact, or `PYTHSCRIBE_MODE=fallback` |

`auto` (the default) picks the first available in that order and records why (`binding.mode_reason`).
Force one with `PYTHSCRIBE_MODE=server|browser|fallback` or `@wasm(mode="server")` — a forced mode that
cannot be honoured raises `ModeError` at import instead of degrading quietly. The mode is resolved once,
at decoration.

## Use cases

In plain CPython, `@wasm` is not competing with JavaScript — it competes with NumPy, Numba and Cython. The
good use cases are the ones where those three lose, and there are more than people assume. The three we
lead with (each is a runnable demo in `examples/wasm-use-cases/`):

1. **Non-vectorisable ML preprocessing / decoding.** Edit distance, DTW, Viterbi / beam-search decoding,
   custom tokenizers, CTC alignment, per-sample augmentation: DP and state machines NumPy cannot express.
   You keep Python semantics (big ints, the MRO) and get the compiled-loop speedup — against *interpreted*
   CPython; we do not claim to beat NumPy or Numba here (see the honest Numba split below — Numba is
   faster on the nopython-eligible loops). The browser instance of this row is the M1 image-preprocess demo
   (`examples/gradio-image-preprocess/`): resize in the tab before upload, 89× fewer bytes aggregate.
2. **Sandboxed (LLM-generated) code.** WebAssembly is a capability sandbox: the server runtime links a
   kernel with *no* WASI — no filesystem, network, clock or environment exists on the path, always — and
   meters execution with fuel **when you ask for it**. `@wasm(fuel=20_000_000)` on a generated snippet is
   the whole story; an unbounded loop traps (`FuelExhausted`), a module that imports file I/O is refused
   before it runs (`SandboxViolation`). Without a fuel budget the server path is unmetered (trusted code
   pays nothing) — so always set one for code you did not write. No seccomp, no containers.
3. **Same function, browser or server (isomorphic).** One `@wasm` function runs in the user's tab (the
   Gradio component) and in-process on the server (wasmtime); the IEEE-754 bit patterns are identical, and
   equal to CPython's. `examples/wasm-use-cases/app.py` shows all three side by side.

**Server side (the pip runtime)**

| Use case | Why `@wasm` wins there | Example |
|---|---|---|
| Non-vectorisable loops — DP, graph walks, state machines, parsers | NumPy can't express it; Numba is faster where it applies but rejects catching a *specific* exception class (see the honest split below); Python semantics kept | edit distance / DTW / Viterbi decoding, tokenizers, CTC alignment |
| Sandboxed execution of untrusted Python | capability sandbox + fuel metering, no ambient I/O | user-submitted or LLM-generated code, plugin systems, grading |
| GIL-free parallelism | each thread gets its own wasmtime instance and the call releases the GIL — no multiprocessing pickling | batch feature extraction, Monte-Carlo, per-request scoring |
| Bit-for-bit determinism across machines | WASM float/int semantics are fixed; no platform/compiler drift | reproducible simulations, hashing/signing, paper artifacts |
| Single-artifact deployment | one `.wasm`, no toolchain at deploy | Lambda/Workers/WASI, air-gapped installs, pinning a hot path independently of the Python version |
| Streaming / protocol codecs, incremental parsers | pure-Python control flow, deterministic | log/PCAP parsers, custom binary formats, tokenizer streams |
| Rules / pricing / scoring engines | auditable, bit-identical, and the *same* function runs client-side | quotes, eligibility, tax/fee logic, risk scores |
| Plugin / extension ABI | third parties ship a `.wasm` your app loads, capability-limited and fuel-metered | user-authored transforms, marketplace extensions |

**Client side (the same function, compiled)**

| Use case | Why |
|---|---|
| Zero-round-trip interactivity | sliders, live previews, validation, pricing logic run in the tab; works offline |
| Preprocessing at the edge of an ML demo | resize/crop/normalise, audio framing, tokenization before upload — cuts bandwidth on GPU boxes |
| Isomorphic logic | one validation/business-rule function, identical client and server |
| Privacy-preserving compute / on-device redaction | data never leaves the tab; PII masked before upload |
| Offline-first tools / PWAs | the function works with no network at all |
| Client-side search / filter / rank / formula | instant, over data already loaded |

**Where it loses — say so on the page:** anything already NumPy-vectorised (that is C already, and you pay
to marshal the data across the boundary — on a million-float reduction `@wasm` is not only hundreds of
times slower than NumPy, it is slower than the interpreter itself), code that crosses the WASM boundary
*per element* (marshaling dominates — batch the call), and code outside the compiler's supported subset (it
stays in CPython). The full-features notebook measures both losses instead of hiding them.

Two notebooks run all of this, with every number computed from the inputs set in the notebook:
[`examples/wasm-use-cases/three_use_cases.ipynb`](../examples/wasm-use-cases/three_use_cases.ipynb) (the three
leads) and [`examples/wasm-use-cases/full_features.ipynb`](../examples/wasm-use-cases/full_features.ipynb)
(every row above, the escape attempts, the fan-out scaling vs a GIL-bound control, the three-engine digest
identity, and the two honest losses).

### `@wasm` vs Numba — the honest split

On the server, the real competitor to `@wasm` is **Numba** (`@njit`), not nothing. Numba is mature,
ships no new toolchain, is free, and on nopython-eligible numeric kernels it is **faster than
`@wasm`** — often by 10–100×. **We do not beat Numba (or NumPy) on vectorised numerics, and never
claim to.** The [Livermore server-row harness](../benchmarks/livermore_server_row/) measures this
directly across all 24 Livermore kernels (CPython / NumPy / Numba / `@wasm`, in one process): Numba
or NumPy is the fastest column on **every** kernel (Numba 23, NumPy 1 — once you `@njit` the helpers
too, see below), and on the smallest kernels `@wasm` is even slower than interpreted CPython
(per-call marshalling). On the
Livermore server row **`@wasm` never wins on speed** — that is the honest number. Speed is a
**supporting** claim, never the headline.

Where `@wasm` wins is a different axis — and even there the numeric-admission edge is narrow:

| Axis | Numba (`@njit`, server) | `@wasm` (server, wasmtime) |
|---|---|---|
| Vectorised / nopython-scalar numeric speed | **wins** (this is its turf) | loses — do not use `@wasm` here |
| **Admission**: catching a *specific* exception class | rejects it (`except ValueError:` → `TypingError`) | runs it in-process — the concrete server win (see below) |
| Determinism across machines | platform/LLVM-dependent | fixed WASM float/int semantics, bit-for-bit |
| Sandboxing untrusted / LLM-generated code | none | capability sandbox + fuel metering, no ambient I/O |
| GIL-free fan-out | needs `nogil`/care | each thread its own wasmtime instance; the call releases the GIL |
| Deployable artifact | recompiles per host | one `.wasm`, no toolchain at deploy; runs the same in the browser (isomorphic) |

**Honest scope of the admission win (measured against Numba 0.61, not asserted).** Numba is more
capable than a one-liner suggests: its nopython mode **accepts** `raise`, bare `except:`, and
`except Exception:`, and it offers `numba.typed.Dict` and `@jitclass`. So the win is **narrow and
specific**: on the wasmtime server path `@wasm` runs a kernel that **`raise`s a built-in exception
and catches it with `except <SpecificClass>:`** — which Numba's nopython frontend *does* reject
(`TypingError`). The end-to-end proof is
[`tests/pythscribe/test_m4_numba_admission.py`](../tests/pythscribe/test_m4_numba_admission.py):
a `try/except ValueError/raise` kernel Numba rejects runs on the server path under a fuel budget,
bit-exact with CPython. **No Livermore kernel is such a case** (jit their helpers and Numba runs all
24), so the Livermore harness reports **ADMISSION wins: 0** — the SPOT is the sole, honest evidence.

> **Operator-raised `ZeroDivisionError` under a handler is REFUSED from the WASM path (fixed, #496).**
> The admission covers the control-flow *shape* plus an *explicit* `raise`. A `ZeroDivisionError` raised
> by an *operator* — `/`, `//`, `%`, `divmod`, or `0 ** -neg`, directly or in a called kernel — inside a
> `try` that catches it would **trap** on the WASM fast path where CPython runs the handler, so such a
> kernel is now **refused from WASM and stays on the JS backend** (which raises a catchable
> `ZeroDivisionError` and runs the handler exactly as CPython does); an explicit `@wasm` on it fails the
> build with a "drop the `@wasm` decorator" hint rather than admitting-then-trapping. (Remaining honest
> limit: `math` *domain* errors — `math.sqrt(-1)`, `math.log(0)` — are NaN/-inf on the WASM path rather
> than a CPython `ValueError`; a separate class. Classes/dict-literals/generators are also not on the
> WASM fast path — they route to JS/browser, an isomorphic story, not a server one.)

Classes, dict literals, and generators are likewise **not** on the WASM/server fast path (they route
to JS/browser) — so they are an *isomorphic/browser* story, never a wasmtime-server admission claim.

**Browser-Numba is a watch-item, not a durable claim.** Numba now compiles *inside a browser-hosted
Python interpreter* (JupyterLite via emscripten-forge) — but it is **scalar-only** today and lives
in a **notebook, not a production app**, and it is **not** on a general Pyodide page yet. So "Numba
can't run in a browser" is stale and we do not say it. What does not expire when Numba lands on
Pyodide is architectural: `@wasm` ships **one small AOT `.wasm`** (vs the Pyodide + llvmlite + LLVM
stack), with cold-start, sandbox, and semantics coverage on its side — those, not browser numeric
speed, are the client-side pitch.

## Numeric arrays — the typed-array ABI

Beyond scalars and `list`s, a `@wasm` kernel can take **fixed-width numeric arrays** —
`Array[dtype]` (1-D) and `Array[dtype, 2]` (2-D, C-contiguous, row-major), for
`dtype ∈ {int32, int64, float32, float64, uint8}`:

```python
from __future__ import annotations   # keeps the bare Array[...] annotation a lazy string
from pythscribe import wasm

@wasm
def scale_rows(a: Array[float64, 2], out: Array[float64, 2], k: float) -> None:
    for i in range(len(a)):
        for j in range(a.shape[1]):
            out[i][j] = a[i][j] * k        # a[i, j] also works; out filled in place (#364: scalar returns)
```

* **Same buffer on the server, in the browser, and in CPython — isomorphic.** The 16-byte ×8
  header + row-major element layout is byte-identical across the server (`pythscribe.runtime`,
  wasmtime), the browser (`pythscribe/ffi/list_buffer.mjs` — the shim the Gradio component imports),
  and the emitted js+wasm glue. The same compiled `.wasm` gives **bit-for-bit** identical output on
  the server and in the browser for `uint8`/`int32`/`int64` (all ops) and for `float64`/`float32`
  **elementwise single-op** (one op per store — IEEE-754 deterministic), both matching CPython/NumPy.
  (`float32` multi-op and float reductions are ≤ a documented tolerance, not bit-exact; a kernel that
  calls a `math.*` host import is ≤ 1 ULP.) On the browser, `int64` crosses as `BigInt64Array`; the
  four other dtypes as their `Number` TypedArray.
* **Bulk-copy, not zero-copy; pass buffers, not elements.** The element region crosses in ONE bulk
  copy each way — `memoryview.tobytes()` in / `memoryview[:] =` back on the server; `TypedArray.set`
  into / out of a view over the WASM `Memory` `ArrayBuffer` on the browser (no intermediate
  serialization, but still one copy each way — it is NOT zero-copy). The win is amortizing ONE
  boundary crossing over a whole buffer — so **pass a buffer and let the kernel loop**, don't cross
  per element.
* **Sound by refusal (the total runtime check).** Before any copy, the actual buffer's dtype / ndim /
  contiguity is checked against the kernel's compiled-for shape; a mismatch (wrong dtype, wrong width,
  strided / Fortran-order, wrong ndim) is **refused loudly** — the server throws, the js+wasm glue
  reroutes to the exact JS twin, the browser shim throws. A wrong-width buffer is never silently
  misread.
* **Where it loses (be honest).** If your data is *already* a vectorized NumPy array and the whole
  operation is one NumPy call, **stay in NumPy** — a boundary crossing plus a Python/JS loop will not
  beat BLAS. The array ABI pays off for **per-element logic NumPy cannot express** (data-dependent
  loops, custom pixel ops) that would otherwise cross the boundary per element — **batch it into one
  buffer**. (The AOT-vs-JIT story — pythscribe's ahead-of-time `.wasm` vs a JIT — is quantified on the
  server side by the [Livermore server-row harness](../benchmarks/livermore_server_row/) —
  CPython / NumPy / Numba / `@wasm` over all 24 kernels; the browser row and the full published grid
  are a separate launch deliverable.)
* **NOT yet admitted: floor-div / bitwise on sub-64-bit (uint8/int32) arrays** — `// % << >> & | ^ ~`
  on a uint8/int32 array kernel is refused, because the fixed-width WRAP of an overflowing
  intermediate would diverge from NumPy; those kernels stay on the JS path (`int64`/`float` array ops
  are unaffected). This is why the image demo's typed-array kernel
  (`examples/gradio-image-preprocess/kernels.py::downscale_nn`, run in the browser via
  `typed_array_nn.html`) is a **nearest-neighbor** downscale (pure `*`/`+` indexing) rather than a
  uint8 box-average (which needs `sum // area`). The box-average `downscale_box` (list-buffer, `int`
  elements = i64) is unchanged and remains the quality reference. Widening sub-64-bit non-ring ops is
  tracked for a later milestone (pythscribe #493).

## `@wasm` reference

```python
@wasm
@wasm(artifact="path/to/__pythscribe__/fn")   # explicit artifact; unusable -> raises, never a silent no-op
@wasm(mode="server" | "browser" | "fallback") # force a mode; unavailable -> ModeError at import
@wasm(fuel=20_000_000)                        # meter the server path (untrusted code); exhausted -> FuelExhausted
```

* **Statically readable, or refused.** `@wasm` must be a literal, unconditional, top-level, sole decorator
  (`@wasm` / `@pythscribe.wasm`), and the function the module's only binding of that name. Aliases,
  stacking, conditional or dynamic application raise `StaticDecorationError` — a silently unrouted
  function is exactly the failure this prevents. The build step reads the same predicate from the AST
  without importing your module.
* **Never compiles on import.** `python -m pythscribe.build module.py` runs the pinned `pyths compile
  --target js+wasm` per kernel and lays `__pythscribe__/<fn>/` beside the source: the `.wasm`, the browser
  entry + glue, a sealed copy of the runtime modules the glue imports, and a sha256 manifest keyed by the
  kernel's source hash. Stale (source changed) or corrupt artifacts fall back to Python with a logged
  warning; commit the directory (a `.gitattributes` is written so line endings never break the hashes).
* **The FFI grammar (what crosses the boundary).** Parameters: `int`, `float`, `bool`, `list[int]`,
  `list[float]`, `list[bool]`, and the numeric arrays `Array[dtype]` / `Array[dtype, 2]` (see
  *Numeric arrays* above); return: `int`, `float`, `bool`, `None`. A kernel that *produces* a buffer
  fills a caller-provided `list` in place and returns a scalar — plain Python semantics; the server path
  reads every list the kernel assigns into back into your list. Ints cross as exact i64 or are refused
  (never rounded or wrapped); an i64 overflow inside the kernel is refused with `OverflowError`. The
  server path is strict about types: `bool` accepts `True`/`False`/`0`/`1` only, `int` accepts Python
  ints only — NumPy scalars (`np.int64`, `np.bool_`) are refused loudly, convert them first.
  Keyword-only / `*args` / `**kwargs` parameters keep the kernel on the Python body (mode `browser` or
  `fallback`) with the reason recorded — and so does any list use the write-back could not honour
  (aliasing a list parameter, rebinding it, passing it to a call, slice assignment, `del`, or
  length-changing methods): refused with the reason, never read back wrongly.
* **Path markers.** `binding_of(fn)` → `.mode`, `.mode_reason`, `.artifact`, `.artifact_status`,
  `.calls()` (Python-body runs), `.server_runs()` (in-process WASM runs), `.run_python(...)` /
  `.run_server(...)` to pick a path explicitly, `.server` (the `ServerKernel`, exposing `host_imports`,
  `wasm_sha256`, `instances`).

## The server runtime and the sandbox (`pythscribe.runtime`)

```python
from pythscribe.runtime import ServerKernel, Sandbox, FuelExhausted, SandboxViolation

k = ServerKernel.from_artifact(binding_of(edit_distance).artifact, sandbox=Sandbox(fuel=50_000_000, memory_bytes=64 << 20))
r = k.call("edit_distance", ["list[int]"] * 4, [a, b, [0] * (len(b) + 1), [0] * (len(b) + 1)], return_type="int")
r.value, r.fuel_used, r.heap_bytes
```

* **No ambient I/O, structurally.** The only host functions the linker ever defines are the compiler's
  16 pure `math.*` f64 imports (with JavaScript semantics, so browser and server agree on the edges:
  `sqrt(-1)` is NaN, `log(0)` is -inf, `exp(1000)` is inf). There is no WASI object on the path; any other
  import is refused at link time with `SandboxViolation`.
* **Fuel — opt-in.** `Sandbox(fuel=N)` / `@wasm(fuel=N)` / `PYTHSCRIBE_FUEL=N` meter every call; an
  unbounded loop traps with `FuelExhausted` in milliseconds. `CallResult.fuel_used` is the cost of a call.
  The default (`fuel=None`) is **unmetered**: an unbounded loop then runs unbounded, so a budget is
  mandatory for untrusted or generated code.
* **Memory cap.** `memory_bytes` bounds linear memory; a list that does not fit is refused before a byte
  is written.
* **Threads.** Every thread gets its own Store + Instance (own memory) of the shared compiled module, and
  the WASM call releases the GIL; `k.new_instance()` gives an isolated instance for one untrusted run.
* **Determinism.** Kernels with no host imports produce identical bits on every engine (the tab's V8, wasmtime,
  and CPython). With `math.*` calls the browser's libm and CPython's platform libm may differ in the last
  ulp for transcendental functions — `k.host_imports` names the dependency.

## Fallback and limits

* **No artifact / no `wasmtime` / a `.pyc`-only deployment:** the Python body runs, always. `import
  pythscribe` needs nothing installed; the adapters lazy-import their framework and tell you `pip install
  gradio` only when you touch the component.
* **The browser adapter crosses one return type** (`float`, transported as its IEEE-754 bits so -0.0 / inf
  / nan survive); the M1 image component and the FFI shim handle list buffers. The server path returns
  int/float/bool/None exactly.
* **Compiler subset.** A kernel must be WASM-eligible (annotated numeric params, scalar return, no
  closures/classes/strings). A `while i < n and xs[i]` guard is not short-circuited by pyths 0.2.4's WASM
  backend (an upstream bug found by this package's own tests); write the guard as a nested `if` until it
  is fixed.
* **Not a Numba replacement for already-vectorised code**, and never "faster ML / GPU". The wins are the
  cases NumPy/Numba/Cython cannot do.

## Optimized artifacts (`wasm-opt`)

The build runs binaryen's `wasm-opt -Os … --enable-mutable-globals` over each kernel when one is discoverable —
on `PATH`, or via `PYTHS_WASM_OPT` (a bare name is searched on `PATH`; anything else must be a truly absolute
path to an existing file, exactly the compiler's own rule; a relative or missing override is refused, never
resolved against the cwd and never silently replaced by a `PATH` fallback — with one sanctioned exception:
`PYTHS_WASM_OPT=<absolute dir>/.no-wasm-opt` is the *optimizer OFF* switch, a silent `none` whose build is
byte-identical to an optimizer-free machine's). The `--enable-mutable-globals` flag is required: compiled
kernels export mutable globals (the FFI's `__ovf`/`__heap_ptr`/`__err_code`), which older binaryen builds
(e.g. Ubuntu's `apt` version_105) refuse without it. There is no `[optimize]` extra in this release. The
manifest records `optimizer: {id, path_sha256, applied, error}` — `id` is the ambient identity
(`none` or `wasm-opt/<version>`), `applied` says whether the pass actually ran **and its output was adopted**.

* **Integrity vs freshness.** Loading an artifact (`@wasm` at import, `resolve()`/`verify()`) is integrity-only
  and never looks at the machine's optimizer: a prebuilt optimized artifact copied to a box with no `wasm-opt`
  is adopted and runs. Only the *build* (`pyths build`, and the compile-on-first-call cache, which is keyed by
  the optimizer id) rebuilds when the optimizer state changed (none→present, present→none, version change).
* **Never in place, published only after validation.** The optimizer writes into a private temp dir beside the
  artifact; the candidate is adopted by one atomic replace only after `wasmtime.Module.validate` accepts the
  final bytes. Any failure keeps the unoptimized `.wasm` (still valid), records `applied: false` + the reason,
  and the build succeeds.
* **Known limitation — validator coupling.** The only validator the Python build has today is `wasmtime`
  (`pip install wasmtime` / `pythscribe[server]`). Without it the optimized output is **never adopted** —
  including for kernels that only ever run in the browser tab. That is a size/perf regression, never a
  correctness one.

## Packaging and licence

`pip install pythscribe` is bundle-all-in-one: the decorator, the build step, the FFI shim, the server
runtime, and both framework adapters with the Gradio custom component **vendored into the wheel** (no
separate component install). `gradio`, `streamlit` and `wasmtime` are **not** base dependencies — they are
heavy and pinned in your app — the extras `[gradio]`, `[streamlit]`, `[server]`, `[all]` only add them.

The pip package is **MIT** (`pythscribe/LICENSE`), like the JS runtime that ends up in your bundle. The
compiler (`pyths`, the Rust crates and npm plugins) is FSL-1.1-ALv2 — and since the wheel **bundles** the
`pyths` binary (`pythscribe/_bin/`), the wheel's licence expression is `MIT AND LicenseRef-FSL-1.1-ALv2`:
its `.dist-info/licenses/` carries the MIT and FSL texts, the vendored runtime's notice, a per-installed-path
map (`pythscribe/LICENSES-MAP.md`) and the third-party notices of the crates linked into the binary
(`third_party/`). The per-component map is [`LICENSING.md`](../LICENSING.md). Compiled output of your own
code is yours.

Development: `pip install -e .[test]`, `python -m playwright install chromium`, build the example artifacts
(`python -m pythscribe.build examples/gradio-wasm/kernels.py examples/gradio-image-preprocess/kernels.py
examples/wasm-use-cases/kernels.py`), then `PYTHSCRIBE_REQUIRE_ORACLE=1 pytest tests/pythscribe` (a missing
oracle — node, the browser, the artifacts — is a failure there, not a skip).
