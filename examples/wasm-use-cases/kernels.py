"""v0.2.5 M1.5 -- the `@wasm` use-case kernels. Ordinary Python, every one; compiled to WASM
by the explicit build step (never on import):

    python -m pythscribe.build examples/wasm-use-cases/kernels.py

Each is a pure-Python loop NumPy cannot express and Numba would reject or restrict, and each
compiles to a `.wasm` with NO host imports (pure i64/f64 arithmetic) -- which is what makes
"same bits in the browser, on the server, and in CPython" true by construction.

Value-domain contract (what "same bits" does and does NOT cover). The bit-for-bit agreement holds
for inputs WITHIN each kernel's valid domain -- the domain its `Arg` transform produces in the demo
(`luhn_ok` via `Arg.digits` = digits 0-9; the image kernels with dims the loader derives from the
bitmap). OUTSIDE that domain the two runtimes diverge in a KNOWN, ONE-DIRECTIONAL way: pyths'
WASM runtime REFUSES i64 overflow and out-of-range memory access -- it TRAPS, surfaced by the
in-tab client as a loud `error (in-tab @wasm, NO fallback)` -- where CPython's arbitrary-precision
ints / list bounds may instead return a value or raise a different error. Examples reachable only
by hand-wiring a wider transform or bad dims: `luhn_ok` bound to `Arg.ints` on `[2**53-1]*1024`
(sum overflows i64 -> WASM traps, CPython returns), or an image kernel given `h` larger than the
bitmap (OOB -> WASM traps, CPython IndexError). The one place a divergence would be SILENT is float
division by a zero denominator (WASM float `/` yields +/-inf, it does NOT trap) -- so
`monthly_payment` GUARDS every zero-denominator term explicitly (see its body) to keep both paths
in exact agreement. The rule the demo relies on: pyths never silently returns a WRONG value; a
domain violation is a loud refusal, and the float-division exception is guarded at the source.

Buffer convention (M1): a kernel that produces a buffer fills an `out`/scratch `list` IN PLACE
and returns one scalar; `prev`/`cur` are caller-provided DP scratch rows (len(b)+1). The
CPython body mutates those lists; so does the server path (it reads them back); so does the
browser shim.

  edit_distance   Levenshtein distance over codepoint lists (non-vectorizable DP)
  dtw_distance    dynamic-time-warping distance between two float series (the isomorphic demo)
  viterbi         max-probability state path through an HMM (log-space; writes `path`)
  mask_digit_runs on-device redaction: mask runs of >= min_run digits (card/phone numbers)
  spin            an unbounded loop -- the sandbox's fuel-trap witness (never call it unmetered)
  sum_squares     an already-vectorizable reduction -- the honest "where it loses" witness
  is_vowel / count_vowels   per-element vs batched boundary crossing (marshaling dominates)
  threshold_lum / sobel     v0.2.6 fix B: IN-TAB image filters over first-class typed arrays
                            (`Array[uint8, 2]` = [H, W*3] row-major RGB), driven from a Gradio
                            `js=` hook (browser_image_filters_app.py) -- no server round-trip
                            for the transform; the filtered pixels are read back from WASM memory
  count_above / luhn_ok / pii_scan / monthly_payment
                            v0.2.6 fix A: the four SCALAR in-tab use cases behind
                            `pythscribe.gradio.client_side` (browser_scalar_client_app.py) --
                            a client-side filter, a Luhn card check, an on-device digit-run
                            (PII) scan, a loan calculator; each `-> float` (M0's one crossed
                            return type), no list is written, so nothing needs reading back

`from __future__ import annotations` keeps the bare `Array[uint8, 2]` annotations lazy strings
at runtime (not Python names); the compiler reads them from the source AST.
"""
from __future__ import annotations

from pythscribe import wasm


@wasm
def edit_distance(a: list[int], b: list[int], prev: list[int], cur: list[int]) -> int:
    n = len(a)
    m = len(b)
    for j in range(m + 1):
        prev[j] = j
    for i in range(1, n + 1):
        cur[0] = i
        for j in range(1, m + 1):
            cost = 1
            if a[i - 1] == b[j - 1]:
                cost = 0
            best = prev[j] + 1
            if cur[j - 1] + 1 < best:
                best = cur[j - 1] + 1
            if prev[j - 1] + cost < best:
                best = prev[j - 1] + cost
            cur[j] = best
        for j in range(m + 1):
            prev[j] = cur[j]
    return prev[m]


@wasm
def dtw_distance(a: list[float], b: list[float], prev: list[float], cur: list[float]) -> float:
    n = len(a)
    m = len(b)
    big = 1e300
    for j in range(m + 1):
        prev[j] = big
    prev[0] = 0.0
    for i in range(1, n + 1):
        cur[0] = big
        for j in range(1, m + 1):
            d = a[i - 1] - b[j - 1]
            if d < 0.0:
                d = -d
            best = prev[j]
            if cur[j - 1] < best:
                best = cur[j - 1]
            if prev[j - 1] < best:
                best = prev[j - 1]
            cur[j] = d + best
        for j in range(m + 1):
            prev[j] = cur[j]
    return prev[m]


@wasm
def viterbi(logp: list[float], trans: list[float], emit: list[float], n_states: int, n_steps: int, score: list[float], back: list[int], path: list[int]) -> float:
    for s in range(n_states):
        score[s] = logp[s] + emit[s]
    for t in range(1, n_steps):
        for s in range(n_states):
            best = -1e300
            arg = 0
            for r in range(n_states):
                v = score[(t - 1) * n_states + r] + trans[r * n_states + s]
                if v > best:
                    best = v
                    arg = r
            score[t * n_states + s] = best + emit[t * n_states + s]
            back[t * n_states + s] = arg
    best = -1e300
    arg = 0
    for s in range(n_states):
        if score[(n_steps - 1) * n_states + s] > best:
            best = score[(n_steps - 1) * n_states + s]
            arg = s
    path[n_steps - 1] = arg
    for t in range(n_steps - 1, 0, -1):
        arg = back[t * n_states + arg]
        path[t - 1] = arg
    return best


@wasm
def mask_digit_runs(codes: list[int], min_run: int, out: list[int]) -> int:
    n = len(codes)
    masked = 0
    i = 0
    while i < n:
        c = codes[i]
        if c >= 48 and c <= 57:
            j = i
            more = 1
            # NOT `while j < n and codes[j] ...`: pyths 0.2.4 evaluates BOTH operands of `and`
            # in the WASM backend (no short-circuit), so `codes[n]` would trap where CPython
            # never reads it -- upstream reproducer in pythscribe/review/2026-09-03-upstream-*.md
            while more == 1:
                if j < n:
                    if codes[j] >= 48 and codes[j] <= 57:
                        j = j + 1
                    else:
                        more = 0
                else:
                    more = 0
            if j - i >= min_run:
                for k in range(i, j):
                    out[k] = 42
                masked = masked + (j - i)
            else:
                for k in range(i, j):
                    out[k] = codes[k]
            i = j
        else:
            out[i] = c
            i = i + 1
    return masked


@wasm
def spin(n: int) -> int:
    i = 0
    while n != 0:
        i = (i + 1) % 1000
    return i


@wasm
def sum_squares(xs: list[float]) -> float:
    acc = 0.0
    for x in xs:
        acc = acc + x * x
    return acc


@wasm
def is_vowel(c: int) -> int:
    if c == 97 or c == 101 or c == 105 or c == 111 or c == 117:
        return 1
    return 0


@wasm
def count_vowels(codes: list[int]) -> int:
    n = 0
    for c in codes:
        if c == 97 or c == 101 or c == 105 or c == 111 or c == 117:
            n = n + 1
    return n


# ---- v0.2.6 fix B: typed-array image filters (browser_image_filters_app.py) ------------------
# `img`/`out` are 2-D uint8 arrays of shape [H, W*3] (row-major RGB bytes: NumPy
# `rgb.reshape(H, W*3)`); `out` is filled IN PLACE and only a scalar pixel count is returned
# (M0 admits scalar returns; the browser reads `out` back through the typed-array FFI shim).
# Only `+ - *` and compares -- no floor-div -- so both admit on the uint8 WASM fast path. The
# threshold compares the channel SUM (0..765) against `thr`; the app passes `thr*3` for a
# per-channel-mean slider. Sobel = |gx| + |gy| of the RED channel, clamped to 255, written to
# all three channels; the 1-px border of `out` is left as the caller filled it (zeros).
@wasm
def threshold_lum(img: Array[uint8, 2], out: Array[uint8, 2], h: int, w: int, thr: int) -> int:
    for y in range(h):
        for x in range(w):
            c = x * 3
            s = img[y][c] + img[y][c + 1] + img[y][c + 2]
            v = 0
            if s > thr:
                v = 255
            out[y][c] = v
            out[y][c + 1] = v
            out[y][c + 2] = v
    return h * w


@wasm
def sobel(img: Array[uint8, 2], out: Array[uint8, 2], h: int, w: int) -> int:
    for y in range(1, h - 1):
        for x in range(1, w - 1):
            c = x * 3
            gx = img[y - 1][c + 3] + 2 * img[y][c + 3] + img[y + 1][c + 3] - img[y - 1][c - 3] - 2 * img[y][c - 3] - img[y + 1][c - 3]
            gy = img[y + 1][c - 3] + 2 * img[y + 1][c] + img[y + 1][c + 3] - img[y - 1][c - 3] - 2 * img[y - 1][c] - img[y - 1][c + 3]
            m = gx
            if m < 0:
                m = -m
            nn = gy
            if nn < 0:
                nn = -nn
            e = m + nn
            if e > 255:
                e = 255
            out[y][c] = e
            out[y][c + 1] = e
            out[y][c + 2] = e
    return (h - 2) * (w - 2)


# ---- v0.2.6 fix A: the four scalar in-tab use cases (browser_scalar_client_app.py) -----------
# Each returns `float` -- M0's ONE crossed scalar return type (`_require_scalar_float_return`,
# shared by `dispatch` and `client_side`) -- and writes no list, so nothing needs reading back.
# The handler-side `Arg` transforms produce the inputs: `Arg.const(DATA)` + `Arg.float` for the
# filter, `Arg.digits` for the card number, `Arg.utf8_bytes` for the scanned text.
@wasm
def count_above(xs: list[float], thr: float) -> float:
    n = 0.0
    for x in xs:
        if x > thr:
            n = n + 1.0
    return n


@wasm
def luhn_ok(digits: list[int]) -> float:
    total = 0
    parity = len(digits) % 2
    for i in range(len(digits)):
        d = digits[i]
        if i % 2 == parity:
            d = d * 2
            if d > 9:
                d = d - 9
        total = total + d
    if total % 10 == 0:
        return 0.0
    return 1.0


@wasm
def pii_scan(text: list[int], min_run: int) -> float:
    runs = 0.0
    cur = 0
    for ch in text:
        if ch >= 48 and ch <= 57:
            cur = cur + 1
            if cur == min_run:
                runs = runs + 1.0
        else:
            cur = 0
    return runs


@wasm
def monthly_payment(principal: float, annual_rate_pct: float, months: int) -> float:
    if months <= 0:
        return 0.0          # degenerate term: a bare `principal / months` (or `/(factor-1)`) is
                            # ZeroDivisionError in CPython but +inf on the WASM float path -- guard
                            # it so both agree (float `/` does NOT trap; see README #496 note)
    r = annual_rate_pct / 1200.0
    if r == 0.0:
        return principal / months
    factor = 1.0
    base = 1.0 + r
    i = 0
    while i < months:
        factor = factor * base
        i = i + 1
    if factor == 1.0:
        return principal / months   # `factor - 1.0` is exactly 0.0 here (r so tiny that `1.0 + r`
                                    # rounded back to 1.0, e.g. rate 1e-300; or a negative rate whose
                                    # powers cycle back to 1.0, e.g. rate -2400%): a bare
                                    # `... / (factor - 1.0)` is ZeroDivisionError in CPython but
                                    # +/-inf on the non-trapping WASM float path. months > 0 here
                                    # (guarded above), so principal/months agrees on both paths.
    return principal * r * factor / (factor - 1.0)
