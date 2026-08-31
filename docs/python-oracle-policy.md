# Python oracle version policy

## The pinned oracle

The CPython **differential oracle** — the reference implementation every
PythScribe program is compared against — is pinned to **CPython 3.14.7**.

PythScribe is an AOT compiler whose correctness claim is *behavioral fidelity
to CPython*: a compiled program's stdout, and its exception kinds and message
strings, must match what CPython produces for the same source. To make that
claim testable, the harness runs a large corpus of Python programs under a live
CPython interpreter and byte-compares PythScribe's output against it. The
version of that interpreter is the **oracle version**, and it is pinned here so
the claim is precise and reproducible.

## CPython is THE oracle — never Transcrypt

The reference is **always CPython**, never any other Python-to-JS compiler's
expected outputs. In particular, **Transcrypt is used only for its test
PROGRAMS** (a large, real-world Python corpus): those programs are run as a
PythScribe-vs-**CPython** differential. Transcrypt's own runtime results are
*not* a source of truth and are never asserted against — where Transcrypt
deviates from CPython, CPython wins. Any golden that traces to a non-CPython
"expected" value is a bug.

## The policy

1. **Track the current stable CPython.** The oracle should be a released,
   stable CPython (not a pre-release, not a distro-patched build).
2. **Bump deliberately.** Bumping the oracle is an explicit, reviewed change on
   its own branch — never a silent floating dependency. CI pins the version
   (`.github/workflows/ci.yml` → `actions/setup-python` `python-version`).
3. **Regenerate goldens from the new version.** Where goldens are computed live
   they auto-adapt; where they are embedded, re-derive them by running the
   generating programs under the new interpreter (never hand-edit a golden).
4. **Triage the churn.** Every diff the new version produces is classified as
   exactly one of:
   - **(a) message reword** — same exception *kind*, same value, only the
     message string changed → update the golden. Mechanical.
   - **(b) genuine behavior change** — a different value or a different
     exception *kind* → do **not** silently "fix"; record it for a human
     decision (it may be a deliberate divergence or a conformance question).
   - **(c) a PythScribe bug the newer oracle exposes** → record it; fix it in
     the normal fidelity-fix campaign, not in the version-bump change.
   Never weaken or delete a test to make the bump green — a real behavior diff
   is reported, not silenced.

## Reproducing the differential against the pinned oracle

**Every live-CPython lane resolves its interpreter through the ONE shared
module `tests/differential/oracle_python.mjs`**, which honors
`PYTHS_ORACLE_PYTHON` (whitespace-split, so launcher forms work) and defaults
to `python` on `PATH`. CI installs the pinned version via
`actions/setup-python`, so `python` there *is* the oracle. Locally, pin it
explicitly with `PYTHS_ORACLE_PYTHON` — it governs ALL of these lanes:

- `tests/differential/run.mjs` — the 1,376-case semantic corpus
- `tests/differential/harness.mjs` — the shared harness behind
  `gen_identifier_cases.mjs` (S1) and `fuzz_gen.mjs` (S2)
- `tests/differential/i64_boundary/run.mjs` — the i64-boundary differential
- `tests/differential/livermore/run.mjs` — the 24 Livermore kernels
- `crates/pyths_runtime/js/format_diff_test.mjs` — the format-spec
  differential
- `crates/pyths_codegen_js/tests/str_method_matrix.rs` — the str-method
  inline≡runtime≡CPython matrix (a Rust test, so it MIRRORS the resolver:
  same `PYTHS_ORACLE_PYTHON` whitespace-split + fallback order, and it
  additionally REQUIRES the resolved interpreter to be exactly 3.14.x and
  FAILS with no oracle — the matrix has no skip switch)

(A lane spawning bare `python` itself would silently escape the pin — new
live-CPython lanes MUST import the resolver.)

```sh
# Build the compiler once.
cargo build --release --bin pyths

# Full 1,376-case CPython semantic differential against the pinned oracle.
#   Windows launcher:
PYTHS_ORACLE_PYTHON="py -3.14" node tests/differential/run.mjs
#   POSIX:
PYTHS_ORACLE_PYTHON=python3.14 node tests/differential/run.mjs

# Format-spec differential (~30 cases; same env-var pin).
PYTHS_ORACLE_PYTHON="py -3.14" node --test crates/pyths_runtime/js/format_diff_test.mjs

# Behavioral differential expressed as a cargo test (embedded goldens; the
# corpus programs were captured from CPython — see each row's provenance).
cargo test -p pyths_codegen_js --test behavioral_differential

# Runtime helper + error-kind/message unit tests (assert CPython-exact strings).
node --test runtime/src/*.test.mjs
cargo test -p pyths_cli --test error_kind_fidelity
```

When these are all green, PythScribe is byte-faithful to CPython 3.14.7 across
the tested surface.

## History

- **3.12 → 3.14.7** (this bump): the value surface was unchanged. Error-message
  rewords picked up from 3.14 (class (a), applied):
  - Every division/modulo `ZeroDivisionError` message was unified upstream to
    the single `"division by zero"` (3.12 distinguished `"integer division or
    modulo by zero"`, `"integer modulo by zero"`, `"float division by zero"`,
    `"float floor division by zero"`, `"float modulo by zero"`,
    `"float divmod()"`). The compiler's old float/int message split (F4) is
    obsolete under 3.14 and now collapses to the unified message.
  - `pow(0, negative)` → `"zero to a negative power"` (was `"0.0 cannot be
    raised to a negative power"`).
  - `in` on a non-container → `"argument of type 'X' is not a container or
    iterable"` (was `"... is not iterable"`).
  - Genuine behavior change recorded (class (b), **not** changed): a walrus
    (`:=`) inside an annotation is a `SyntaxError` in 3.14
    ("named expression cannot be used within an annotation"); PythScribe still
    accepts and evaluates it (3.12 semantics). See the `def_time_eval_matches_cpython`
    test's oracle note.
