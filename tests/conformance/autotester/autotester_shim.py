# AutoTester shim — a pyths-compatible + CPython-compatible reimplementation of
# Transcrypt's `org.transcrypt.autotester.AutoTester`, adapted for an
# accumulate-then-oracle-once CPython differential.
#
# Original convention (Transcrypt, Apache-2.0): each testlet defines
# `run(autoTester)` and calls `autoTester.check(*args)` many times, then the
# harness calls `done()`. Transcrypt compares CPython vs transpiled JS in a
# browser DIV. Here we instead:
#   * `check(*args)`  -> append ONE normalized transcript line = repr(tuple(args))
#   * `done()`        -> print the whole transcript + a sentinel
# so a single testlet yields a single deterministic transcript that we run
# identically under CPython (oracle) and under `pyths run` (subject), then diff
# ONCE (see autotester_ps.py). repr(tuple(args)) is `eval`/literal-parseable, so
# the runner's by-design normalizers (float/int repr unification, set/dict
# internal order — reused from experiments/pbt-ps/differential.py) apply cleanly.
#
# This file is INLINED (concatenated) ahead of every ported testlet by the
# runner, so the exact same source text is what both CPython and pyths execute.
# It is deliberately written in the plain-Python subset pyths accepts.

SENTINEL = "__PS_PASS__"


class _Envir:
    # Transcrypt's __envir__: testlets branch on
    # `executor_name == transpiler_name` (JS) vs else (CPython). We force BOTH
    # CPython and pyths onto the NON-transpiler (Python-semantics) branch, so the
    # same code path runs on both sides of the differential.
    def __init__(self):
        self.executor_name = "cpython"
        self.transpiler_name = "transcrypt"
        self.interpreter_name = "cpython"


__envir__ = _Envir()
__symbols__ = []


def __pragma__(*args):
    # Transcrypt compiler directive ('opov', 'kwargs', 'iconv', 'js', ...).
    # A no-op under standard Python semantics. NOTE: `__pragma__('js', ...)`
    # raw-JS injection is therefore silently dropped — testlets that depend on it
    # (metaclasses/proxies/async) are handled/skipped explicitly by the runner.
    return None


def __new__(x):
    # Transcrypt's JS-`new` helper; identity under Python semantics.
    return x


class AutoTester:
    def __init__(self, symbols=None):
        self.symbols = symbols if symbols is not None else []
        self._lines = []

    def _norm_arg(self, x):
        # Documented by-design normalizer, mirroring Transcrypt's own AutoTester
        # (`type(any) == range: return repr(list(any))`): PythScribe compiles a
        # lazy `range` to an eager JS array, so `range(0, 32)` reprs as a list.
        # Convert range -> list on the CPython side (under pyths it is already a
        # list, so isinstance(range) is False and this is a no-op) so both sides
        # agree. Recurse into list/tuple so nested ranges normalize too.
        if isinstance(x, range):
            return [self._norm_arg(e) for e in x]
        if isinstance(x, list):
            return [self._norm_arg(e) for e in x]
        if isinstance(x, tuple):
            return tuple(self._norm_arg(e) for e in x)
        return x

    def check(self, *args):
        # One transcript line per check: a tuple-repr of all args. Tuple form is
        # uniform (1 arg -> `(x,)`), always a single eval-parseable expression,
        # and lets the runner's structural normalizers see through set/dict order
        # and int-vs-float repr.
        self._lines.append(repr(tuple(self._norm_arg(a) for a in args)))

    def expectException(self, func):
        # Error-OCCURRENCE oracle.
        try:
            func()
            return "no exception"
        except Exception:
            return "exception"

    def throwToError(self, func):
        # Error-KIND oracle: on failure record the exception CLASS name, NOT the
        # message (messages are impl-specific; the class is the conformance
        # signal). Transcrypt embedded str(exc); we deliberately do not, so
        # pyths's differing wording is not counted as a divergence.
        try:
            return func()
        except Exception as exc:
            return (None, "!!!" + type(exc).__name__)

    def checkEval(self, func):
        self.check(self.throwToError(func))

    def checkPad(self, val, count):
        for _i in range(count):
            self.check(val)

    def run_testlet(self, fn, name):
        # Mirror Transcrypt AutoTester.run: invoke the testlet, but record a
        # crash as a transcript line so a one-sided crash surfaces as a
        # divergence (rather than a silent stop) in the diff.
        try:
            fn(self)
        except Exception as exc:
            self._lines.append("__TESTLET_EXC__:" + type(exc).__name__)

    def done(self):
        out = []
        for ln in self._lines:
            out.append(ln)
        out.append(SENTINEL)
        print("\n".join(out))
