"""v0.2.6 fix A -- `pythscribe.gradio.client_side` / `Arg`: the in-tab SCALAR client. No browser:
the spec is derived from the kernels' statically-read signatures and the artifact URL; the
contract (`-> float`, no written list, no typed array), the Arg-to-parameter mapping and every
const are REFUSED (never guessed) when outside it; the emitted loader / handler JS is pinned; and a
Node run of the REAL emitted handler + client + shim + .wasm is compared bit-for-bit with the
kernels' CPython bodies (the browserless differential; gated on node, RED under
PYTHSCRIBE_REQUIRE_ORACLE when node is missing)."""
from __future__ import annotations

import json
import random
import re
import subprocess

import pytest

from conftest import REPO, gate, gate_import, gate_node, import_module_from

gate_import("gradio")  # `wasm_url` registers a Gradio static path

USE_CASES = REPO / "examples" / "wasm-use-cases"

from pythscribe.ffi import SHIM  # noqa: E402
from pythscribe.gradio import Arg, ClientSide, client_side  # noqa: E402  (the package re-export)
from pythscribe.gradio.browser import SCALAR_CLIENT_JS, browser_image_loader_js, scalar_kernel_spec  # noqa: E402

_SEP = chr(0x2028), chr(0x2029)  # (never typed literally: line-wise tooling splits on them -- opus fix-B r2/NEW-2)


@pytest.fixture(scope="module")
def mod():
    for fn in ("count_above", "luhn_ok", "pii_scan", "monthly_payment"):
        assert (USE_CASES / "__pythscribe__" / fn / "manifest.json").is_file(), f"build examples/wasm-use-cases/kernels.py ({fn})"
    return import_module_from(USE_CASES / "kernels.py", "kernels_uc_for_client_side")  # by path: never shadow another `kernels`


@pytest.fixture(scope="module")
def cs(mod) -> ClientSide:
    return client_side({"filter": mod.count_above, "luhn": mod.luhn_ok, "pii": mod.pii_scan, "loan": mod.monthly_payment})


def test_spec_of_the_four_use_cases(mod, cs):
    s = scalar_kernel_spec(mod.monthly_payment)
    assert s["fn"] == "monthly_payment" and s["return_type"] == "float"
    assert s["param_types"] == ["float", "float", "int"] and s["params"] == ["principal", "annual_rate_pct", "months"]
    assert s["wasm"].startswith("/gradio_api/file=") and s["wasm"].endswith("/monthly_payment.wasm")
    assert cs.kernels == ["filter", "luhn", "pii", "loan"]
    assert cs.spec["luhn"]["param_types"] == ["list[int]"] and cs.spec["pii"]["param_types"] == ["list[int]", "int"]
    assert cs.spec["filter"]["param_types"] == ["list[float]", "float"]
    one = client_side(mod.luhn_ok)  # a single function: its own name
    assert one.kernels == ["luhn_ok"] and one.global_name == "pythscribeScalar"


def _wasm(src: str, name: str):
    """A kernel whose SOURCE is read statically by the decorator (needs a real file)."""
    import tempfile
    import textwrap
    from pathlib import Path

    f = Path(tempfile.mkdtemp()) / f"{name}_mod.py"
    f.write_text("from __future__ import annotations\nfrom pythscribe import wasm\n" + textwrap.dedent(src), encoding="utf-8")
    return getattr(import_module_from(f), name)


@pytest.mark.parametrize("src,name,msg", [
    # the ONE M0 return authority (shared with dispatch): int / bool / None returns are refused
    ("@wasm\ndef k(x: int) -> int:\n    return x\n", "k", "must return float in M0"),
    ("@wasm\ndef k(x: int) -> bool:\n    return x > 0\n", "k", "must return float in M0"),
    ("@wasm\ndef k(x: int) -> None:\n    return None\n", "k", "must return float in M0"),
    # nothing is read back on this path: a written list is refused, never silently lost
    ("@wasm\ndef k(xs: list[int], out: list[int]) -> float:\n    out[0] = xs[0]\n    return 1.0\n", "k", "writes into ['out']"),
    ("@wasm\ndef k(a: list[float], prev: list[float], cur: list[float]) -> float:\n    prev[0] = 0.0\n    cur[0] = 1.0\n    return prev[0]\n", "k", "writes into ['cur', 'prev']"),
    # typed arrays are the image contract's
    ("@wasm\ndef k(img: Array[uint8, 2], h: int) -> float:\n    return 1.0\n", "k", "is a typed array"),
])
def test_contract_refusals(src, name, msg):
    with pytest.raises(TypeError, match=re.escape(msg)):
        scalar_kernel_spec(_wasm(src, name))


def test_contract_positive_control():
    """The paired positive: the same shapes WITHOUT the offending feature are admitted (so the
    refusals above are discriminating, not a blanket refusal)."""
    from pythscribe import binding_of
    from pythscribe.decorators import NO_JIT_ENV
    import os

    fn = _wasm("@wasm\ndef ok(xs: list[int], k: int, f: float, b: bool) -> float:\n    n = 0.0\n    for x in xs:\n        if x > k:\n            n = n + f\n    if b:\n        n = n + 1.0\n    return n\n", "ok")
    b = binding_of(fn)
    assert b.mutated_params == frozenset() and b.return_type == "float"
    # contract passes; only the artifact is missing (compile-on-first-call is OFF in the suite) -> RuntimeError, never a fallback
    assert os.environ.get(NO_JIT_ENV) == "1"
    with pytest.raises(RuntimeError, match="no usable artifact"):
        scalar_kernel_spec(fn)
    assert b.artifact is None, "the refusal must not have compiled anything"


def test_client_side_refuses_bad_inputs(mod):
    with pytest.raises(ValueError, match="no kernels"):
        client_side({})
    with pytest.raises(ValueError, match="not a JS identifier"):
        client_side(mod.luhn_ok, global_name="not a name")
    with pytest.raises(ValueError, match="identifier-like"):
        client_side({"bad key!": mod.luhn_ok})
    with pytest.raises(TypeError, match="pass a @wasm function"):
        client_side("luhn_ok")  # type: ignore[arg-type]


@pytest.mark.parametrize("args,kernel,msg", [
    ([Arg.float], "luhn", "cannot be produced by Arg.float (which yields `float`)"),
    ([Arg.digits, Arg.float], "luhn", "takes 1 parameter(s)"),
    ([], "luhn", "takes 1 parameter(s)"),
    ([Arg.float, Arg.float, Arg.float], "loan", "parameter `months: int` cannot be produced by Arg.float"),
    ([Arg.float, Arg.float, Arg.digits], "loan", "parameter `months: int` cannot be produced by Arg.digits"),
    ([Arg.const([1.5])], "luhn", "cannot take the const [1.5]"),
    ([Arg.const([1, True])], "luhn", "cannot take the const [1, True]"),
    ([Arg.const(True)], "luhn", "cannot take the const True"),
    ([Arg.utf8_bytes, Arg.const(2.0)], "pii", "cannot take the const 2.0"),
    ([Arg.utf8_bytes, Arg.const(True)], "pii", "cannot take the const True"),
    ([Arg.const(1), Arg.float], "filter", "cannot take the const 1"),
    ([Arg.const([0.5, "x"]), Arg.float], "filter", "cannot take the const"),
    ([Arg.js("v => v", produces="float")], "luhn", "cannot be produced by Arg.js('v => v') (which yields `float`)"),
    (["digits"], "luhn", "args must be a list of Arg"),
    (Arg.digits, "luhn", "args must be a list of Arg"),
])
def test_call_js_refuses_a_mismatched_arg_spec(cs, args, kernel, msg):
    with pytest.raises(TypeError, match=re.escape(msg)):
        cs.call_js(args, kernel=kernel)


def test_call_js_refuses_a_non_crossable_const(cs):
    with pytest.raises(ValueError, match="non-finite"):
        cs.call_js([Arg.const([float("inf")]), Arg.float], kernel="filter")
    with pytest.raises(ValueError, match="exceeds JS Number precision"):
        cs.call_js([Arg.utf8_bytes, Arg.const(2**60)], kernel="pii")


def test_call_js_kernel_selection(cs, mod):
    with pytest.raises(ValueError, match="kernel= is required"):
        cs.call_js([Arg.digits])
    with pytest.raises(ValueError, match="unknown kernel 'nope'"):
        cs.call_js([Arg.digits], kernel="nope")
    assert "invoke(\"luhn_ok\"" in client_side(mod.luhn_ok).call_js([Arg.digits])  # a single kernel needs no kernel=


def test_arg_js_and_format_keep_the_emitted_handler_invariants(cs):
    with pytest.raises(ValueError, match="non-empty JS function"):
        Arg.js("   ")
    with pytest.raises(ValueError, match="</script"):
        Arg.js("v => '</script><script>alert(1)</script>'")
    with pytest.raises(ValueError, match="U\\+2028"):
        Arg.js("v => '" + _SEP[0] + "'")
    with pytest.raises(ValueError, match="not a scalar-client parameter type"):
        Arg.js("v => v", produces="Array[uint8, 2]")
    with pytest.raises(ValueError, match="</script"):
        cs.call_js([Arg.digits], kernel="luhn", format="v => '</SCRIPT>'")
    with pytest.raises(ValueError, match="non-empty JS function"):
        cs.call_js([Arg.digits], kernel="luhn", format="")
    # an unchecked Arg.js is admitted for any parameter (the author's responsibility, documented)
    js = cs.call_js([Arg.js("v => v.split('').map(Number)")], kernel="luhn")
    assert "{t: 'js', f: (v => v.split('').map(Number))}" in js
    assert repr(Arg.js("v => v")) == "Arg.js('v => v')" and repr(Arg.const(3)) == "Arg.const(3)" and repr(Arg.digits) == "Arg.digits"


def test_call_js_emits_the_handler(cs):
    js = cs.call_js([Arg.const([0.1, 0.9]), Arg.float], kernel="filter", format="(v, i) => v + ' above ' + i.args[1]", status=True)
    assert js.startswith("(...a) => {") and js.rstrip().endswith("}")
    assert "const c = window.pythscribeScalar;" in js and "if (!(c && c.ready))" in js
    assert "'loading the in-tab @wasm client...'" in js and "'in-tab @wasm client failed to load (NO fallback): ' + c.loadError" in js
    assert "return c.invoke(\"filter\", a, [{t: 'const', v: [0.1, 0.9]}, {t: \"float\"}], ((v, i) => v + ' above ' + i.args[1]), true);" in js
    js2 = cs.call_js([Arg.utf8_bytes, Arg.const(9)], kernel="pii")
    assert "return c.invoke(\"pii\", a, [{t: \"utf8_bytes\"}, {t: 'const', v: 9}], null, false);" in js2
    for j in (js, js2):
        assert "</script>" not in j.lower() and _SEP[0] not in j and _SEP[1] not in j


def _components():
    """Three REAL Gradio components (the named form refuses non-components; see blocker 2)."""
    import gradio as gr

    return gr.Number(label="principal"), gr.Slider(0, 15, label="rate"), gr.Slider(12, 360, label="months")


def test_call_js_named_form_orders_inputs_by_signature(cs):
    """blocker 1: the NAMED `{param: Arg.const | (Arg, component)}` form returns `(js, inputs)` with
    `inputs` in the kernel's SIGNATURE order, whatever order the mapping is written in -- so a
    component can never be bound to the wrong parameter (the positional-shift class, closed by
    construction). The emitted js is identical to the equivalent positional call."""
    P, R, M = _components()  # real Gradio components (identity-checked below)
    # mapping written in a DELIBERATELY SCRAMBLED order -- months, then principal, then rate
    js, inputs = cs.call_js({"months": (Arg.int, M), "principal": (Arg.float, P), "annual_rate_pct": (Arg.float, R)}, kernel="loan")
    assert inputs == [P, R, M], "inputs must be reordered into signature order (principal, annual_rate_pct, months)"
    # byte-identical to the positional form (the named form is pure sugar over the same _emit_js)
    assert js == cs.call_js([Arg.float, Arg.float, Arg.int], kernel="loan")
    # a const parameter contributes NO component and stays out of inputs; the rest keep their order
    js2, inputs2 = cs.call_js({"principal": (Arg.float, P), "annual_rate_pct": Arg.const(0.0), "months": (Arg.int, M)}, kernel="loan")
    assert inputs2 == [P, M] and js2 == cs.call_js([Arg.float, Arg.const(0.0), Arg.int], kernel="loan")


def test_call_js_named_form_refuses_mis_specified_mappings(cs, mod):
    """blocker 1 + blocker 2: the named form REFUSES an incomplete / over-complete mapping and a
    wrongly-shaped value -- a missing or extra parameter, a bare consuming Arg (needs a component),
    an `Arg.const` given a component, a non-(Arg, component) value, and (blocker 2) a pair whose
    second element is NOT a real Gradio component (`None` or an arbitrary object) -- which would
    otherwise flow into `inputs=[...]` and blow up only later inside Gradio. Nothing is silently
    accepted."""
    P, R, M = _components()
    good = {"principal": (Arg.float, P), "annual_rate_pct": (Arg.float, R), "months": (Arg.int, M)}
    cs.call_js(good, kernel="loan")  # the paired positive control: the complete mapping is accepted
    with pytest.raises(TypeError, match="must cover EXACTLY the parameters"):
        cs.call_js({"principal": (Arg.float, P), "annual_rate_pct": (Arg.float, R)}, kernel="loan")  # missing months
    with pytest.raises(TypeError, match="must cover EXACTLY the parameters"):
        cs.call_js(dict(good, extra=(Arg.float, _components()[0])), kernel="loan")  # an extra name
    with pytest.raises(TypeError, match="maps to a bare"):
        cs.call_js(dict(good, months=Arg.int), kernel="loan")  # a consuming Arg with no component
    with pytest.raises(TypeError, match="takes no component"):
        cs.call_js(dict(good, months=(Arg.const(12), M)), kernel="loan")  # Arg.const given a component
    with pytest.raises(TypeError, match="must map to"):
        cs.call_js(dict(good, months=object()), kernel="loan")  # a bare non-Arg value
    with pytest.raises(TypeError, match="must map to"):
        cs.call_js(dict(good, months=(Arg.int, M, "extra")), kernel="loan")  # a 3-tuple
    # blocker 2: a consuming Arg paired with a NON-value-component -- None, an arbitrary object, or
    # a LAYOUT container (gr.Row is a Block but carries no value) -- is refused HERE, not deferred
    import gradio as gr
    with pytest.raises(TypeError, match="must be a Gradio value component"):
        cs.call_js(dict(good, months=(Arg.int, None)), kernel="loan")
    with pytest.raises(TypeError, match="must be a Gradio value component"):
        cs.call_js(dict(good, months=(Arg.int, object())), kernel="loan")
    with pytest.raises(TypeError, match="must be a Gradio value component"):
        cs.call_js(dict(good, months=(Arg.int, gr.Row(render=False))), kernel="loan")  # a layout, not a value
    # a single-kernel client resolves the kernel implicitly in the named form
    one = client_side(mod.monthly_payment)
    P2, R2, M2 = _components()
    _js, _inputs = one.call_js({"principal": (Arg.float, P2), "annual_rate_pct": (Arg.float, R2), "months": (Arg.int, M2)})
    assert _inputs == [P2, R2, M2]


def test_node_named_form_computes_correctly_despite_scrambled_mapping(cs, mod, tmp_path):
    """blocker 1, end-to-end: a handler from a SCRAMBLED named mapping computes the SAME float as
    CPython WHEN the emitted `js` is fed the values in the RETURNED `inputs` order -- and the
    control proves the returned order is load-bearing. Each component carries a distinct sentinel
    value; `feed = [vals[c] for c in inputs]` is scrambled IFF `_resolve_named` mis-orders `inputs`,
    so a mis-ordering makes the `named` arm DIVERGE. PAIRED CONTROL `shifted`: the same values in the
    mapping's (wrong) WRITTEN order through a positional handler compute a different number."""
    from pythscribe import binding_of
    from pythscribe.build.runner import float_bits

    principal, rate, months = 250000.0, 7.0, 180  # rate integral so every value is valid in any slot
    P, R, M = _components()
    vals = {id(P): principal, id(R): rate, id(M): months}
    # mapping in scrambled written order; feed strictly by the RETURNED inputs' component identities
    js, inputs = cs.call_js({"months": (Arg.int, M), "principal": (Arg.float, P), "annual_rate_pct": (Arg.float, R)}, kernel="loan")
    feed = [vals[id(c)] for c in inputs]   # correct order ONLY if _resolve_named returns signature order
    cases = [
        {"key": "named", "handler": js, "inputs": feed},
        # the paired control: the values in the mapping's WRITTEN order (months, principal, rate)
        # through a POSITIONAL handler -> monthly_payment(180.0, 250000.0, 7) -> a different number
        {"key": "shifted", "handler": cs.call_js([Arg.float, Arg.float, Arg.int], kernel="loan"), "inputs": [months, principal, rate]},
    ]
    got = _node_run(cs, mod, cases, tmp_path)
    want = float_bits(binding_of(mod.monthly_payment).run_python(principal, rate, months))
    assert feed == [principal, rate, months], "sanity: the returned inputs are in signature order"
    assert got["named"]["bits"] == want, "the named handler, fed by returned-inputs order, computes monthly_payment(principal, rate, months)"
    assert got["shifted"]["bits"] != want, "the control (written-order positional) diverges -- the reorder is what makes named correct"


def test_loader_js_inlines_shim_client_and_spec(cs):
    js = cs.loader_js
    assert js.startswith("async () => {") and js.rstrip().endswith("}")
    m = re.search(r'LAYOUT_VERSION\s*=\s*"([^"]+)"', SHIM.read_text(encoding="utf-8"))
    assert m and json.dumps(m.group(1))[1:-1] in js, "the shim's source (with its list layout version) must be inlined"
    assert "function makeScalarClient(ffi, spec)" in js and SCALAR_CLIENT_JS.is_file()
    assert "window.pythscribeScalar = makeScalarClient(ffi, mergeScalarSpec(window.pythscribeScalar, " in js and "window.pythscribeScalar.ready = true;" in js
    call = js.rindex("mergeScalarSpec(window.pythscribeScalar, ") + len("mergeScalarSpec(window.pythscribeScalar, ")  # the CALL (the definition comes first)
    spec = json.loads(js[call:js.index("));\n    window.pythscribeScalar.ready", call)])
    assert set(spec) == {"filter", "luhn", "pii", "loan"} and spec["luhn"]["fn"] == "luhn_ok" and spec["loan"]["return_type"] == "float"
    # the stub is published SYNCHRONOUSLY before the first await, a load failure lands in loadError, the blob URL is revoked
    stub = js.index("window.pythscribeScalar = { ready: false, loadError: null }")
    assert stub < js.index("await import(__shimUrl)") and "loadError: msg" in js and "URL.revokeObjectURL(__shimUrl)" in js
    # ONE `resolveUrl` (S-r3-1): the shim EXPORTS it once (single source, under the byte-identity
    # gate) and the hook aliases it into the client closure exactly once (`const resolveUrl =
    # ffi.resolveUrl;`); the old inlined `_RESOLVE_URL_JS` Python-string copy is gone; the client
    # file defines none of its own.
    assert js.count("const resolveUrl = ffi.resolveUrl;") == 1 and js.count("export function resolveUrl(") == 1
    assert "const resolveUrl = (u) =>" not in js  # the un-gated inlined copy is deleted
    assert "resolveUrl(" in SCALAR_CLIENT_JS.read_text(encoding="utf-8")
    assert "function resolveUrl" not in SCALAR_CLIENT_JS.read_text(encoding="utf-8")
    assert "</script>" not in js.lower() and _SEP[0] not in js and _SEP[1] not in js


def test_image_loader_shares_the_hook_and_its_single_resolve_url(mod):
    """The fix-B image loader now emits through the SAME `_loader_hook`: its own invariants hold
    (pinned in test_browser_image_loader.py) and it carries exactly one `resolveUrl` -- the hook's."""
    from pythscribe.gradio.browser import CLIENT_JS

    uc = import_module_from(USE_CASES / "kernels.py", "kernels_uc_for_client_side_image")
    js = browser_image_loader_js({"sobel": uc.sobel})
    assert js.count("const resolveUrl = ffi.resolveUrl;") == 1 and js.count("export function resolveUrl(") == 1
    assert "const resolveUrl = (u) =>" not in js and "function resolveUrl" not in CLIENT_JS.read_text(encoding="utf-8")
    assert "window.pythscribeImage = { ready: false, loadError: null, filter: async () =>" in js


# ---- the browserless differential: the REAL emitted handler + client + shim + .wasm under Node vs CPython
_NODE_RUNNER = """
import * as ffi from {shim_url};
const resolveUrl = (u) => u;
{client_src}
const spec = {spec};
const c = makeScalarClient(ffi, spec);
c.ready = true;  // what the load hook does after binding
globalThis.window = {{ pythscribeScalar: c }};
const cases = JSON.parse(await (await import('node:fs/promises')).readFile(process.argv[2], 'utf-8'));
const out = [];
for (const k of cases) {{
  const h = (0, eval)('(' + k.handler + ')');   // the handler exactly as Gradio would evaluate it
  const r = await h(...k.inputs);
  out.push({{ key: k.key, result: r, bits: c.last ? c.last.bits : null, value: c.last ? c.last.value : null, lastError: c.lastError, calls: c.calls }});
}}
console.log(JSON.stringify(out));
"""


def _node_run(cs: ClientSide, mod, cases: list[dict], tmp_path):
    from pythscribe import binding_of

    node = gate_node()
    spec = {k: dict(v, wasm=binding_of(getattr(mod, v["fn"])).artifact.wasm.resolve().as_posix()) for k, v in cs.spec.items()}
    runner = tmp_path / "runner.mjs"
    runner.write_text(_NODE_RUNNER.format(shim_url=json.dumps(SHIM.resolve().as_uri()), client_src=SCALAR_CLIENT_JS.read_text(encoding="utf-8"),
                                          spec=json.dumps(spec)), encoding="utf-8")
    (tmp_path / "cases.json").write_text(json.dumps(cases), encoding="utf-8")
    p = subprocess.run([node, str(runner), str(tmp_path / "cases.json")], capture_output=True, text=True, encoding="utf-8", timeout=120)
    assert p.returncode == 0, p.stderr[-3000:]
    return {r["key"]: r for r in json.loads(p.stdout.strip().splitlines()[-1])}


def test_node_differential_bit_for_bit_vs_cpython(cs, mod, tmp_path):
    """Random inputs through the SAME client code path the tab runs (the emitted `call_js`
    handler evaluated under Node with the real shim + .wasm): every float's IEEE-754 bits equal
    the kernel's CPython body on Python twins of the transforms. PAIRED CONTROL: a neighbouring
    input's reference must NOT equal (the equality is discriminating)."""
    from pythscribe import binding_of
    from pythscribe.build.runner import float_bits

    rng = random.Random(11)
    data = [rng.random() for _ in range(300)]
    cases, refs, ctrl = [], {}, {}
    h_filter = cs.call_js([Arg.const(data), Arg.float], kernel="filter")
    h_luhn = cs.call_js([Arg.digits], kernel="luhn")
    h_pii = cs.call_js([Arg.utf8_bytes, Arg.const(4)], kernel="pii", status=True)
    h_loan = cs.call_js([Arg.float, Arg.float, Arg.int], kernel="loan", format="v => 'pay ' + v.toFixed(3)")
    for i in range(8):
        t = rng.random()
        cases.append({"key": f"f{i}", "handler": h_filter, "inputs": [t]})
        refs[f"f{i}"] = float_bits(binding_of(mod.count_above).run_python(data, t))
        ctrl[f"f{i}"] = float_bits(binding_of(mod.count_above).run_python(data, t + 0.07))
        digits = [rng.randrange(10) for _ in range(rng.randrange(1, 20))]
        card = " ".join("".join(map(str, digits[j:j + 4])) for j in range(0, len(digits), 4))
        cases.append({"key": f"l{i}", "handler": h_luhn, "inputs": [card]})
        refs[f"l{i}"] = float_bits(binding_of(mod.luhn_ok).run_python(digits))
        # the control must FLIP validity (Luhn yields only 0.0/1.0): exactly one check digit makes a number valid
        ref_v = binding_of(mod.luhn_ok).run_python(digits)
        [flipped] = [digits[:-1] + [d] for d in range(10) if binding_of(mod.luhn_ok).run_python(digits[:-1] + [d]) != ref_v][:1]
        ctrl[f"l{i}"] = float_bits(binding_of(mod.luhn_ok).run_python(flipped))
        txt = "".join(rng.choice("ab 0123456789é½") for _ in range(rng.randrange(0, 40)))
        cases.append({"key": f"p{i}", "handler": h_pii, "inputs": [txt]})
        refs[f"p{i}"] = float_bits(binding_of(mod.pii_scan).run_python(list(txt.encode("utf-8")), 4))
        # exercises the `months <= 0` guard ONLY (a policy return of 0.0 for a degenerate term):
        # i==0 zero, i==1 negative -> both paths 0.0 bit-for-bit; i>=2 the normal amortization
        # branch. (The OTHER zero-denominator, `factor == 1.0`, is covered by its own paired test
        # `test_node_monthly_payment_zero_denominator_guard_is_exercised` -- these m-cases do NOT
        # reach it: random rates here don't drive factor back to exactly 1.0.)
        months = 0 if i == 0 else (-7 if i == 1 else rng.randrange(1, 400))
        rate = rng.choice([0.0, rng.uniform(0.1, 20.0)])
        cases.append({"key": f"m{i}", "handler": h_loan, "inputs": [rng.uniform(1000, 1e6), rate, months]})
        refs[f"m{i}"] = float_bits(binding_of(mod.monthly_payment).run_python(cases[-1]["inputs"][0], rate, months))
        # the control uses a POSITIVE term so it is non-degenerate (a real payment) -> diverges from
        # both the 0.0 guard cases AND the normal cases
        ctrl[f"m{i}"] = float_bits(binding_of(mod.monthly_payment).run_python(cases[-1]["inputs"][0], rate, max(months, 0) + 1))
    got = _node_run(cs, mod, cases, tmp_path)
    bad = {k: (got[k]["bits"], refs[k], got[k]["lastError"]) for k in refs if got[k]["bits"] != refs[k] or got[k]["lastError"]}
    assert not bad, f"Node client != CPython: {bad}"
    assert refs["m0"] == float_bits(0.0) and got["m0"]["bits"] == refs["m0"], "months == 0 guard: both paths return 0.0 bit-for-bit"
    assert refs["m1"] == float_bits(0.0) and got["m1"]["bits"] == refs["m1"], "months < 0 guard: both paths return 0.0 bit-for-bit (SF4)"
    assert len(got) == len(refs) == 32
    assert all(got[k]["bits"] != ctrl[k] for k in ctrl), "a neighbouring input must diverge (control not vacuous)"
    # the handler's OUTPUT contract: raw Number, `format`'s string, `[value, status]` with status=True
    assert isinstance(got["f0"]["result"], (int, float)) and got["f0"]["result"] == got["f0"]["value"]
    assert got["m0"]["result"] == f"pay {got['m0']['value']:.3f}"
    r = got["p0"]["result"]
    assert isinstance(r, list) and len(r) == 2 and "@wasm in-tab" in r[1] and "0 round-trips" in r[1] and "(call " in r[1]
    assert got["m7"]["calls"] == 32


def test_node_monthly_payment_zero_denominator_guard_is_exercised(cs, mod, tmp_path):
    """PAIRED CONTROL for the `factor == 1.0` guard (the SECOND zero-denominator, distinct from the
    `months <= 0` one the differential covers). `factor - 1.0` is exactly 0.0 when `1.0 + r` rounds
    back to 1.0 (rate ~1e-300, months > 0) or a negative rate's powers cycle factor to 1.0 (rate
    -2400%): a bare `... / (factor - 1.0)` is ZeroDivisionError in CPython but a SILENT +/-inf on
    the WASM float path. This drives those exact inputs THROUGH the Node/WASM handler and asserts
    both paths return `principal / months` bit-for-bit. If the guard were reverted, CPython would
    raise ZeroDivisionError (refs would fail to build) AND the WASM arm would return inf (bits
    mismatch) -- RED either way. The `norm` control (a normal rate, same principal/months) must
    DIVERGE, so the equality is discriminating rather than 'everything is principal/months'."""
    from pythscribe import binding_of
    from pythscribe.build.runner import float_bits

    b = binding_of(mod.monthly_payment)
    h_loan = cs.call_js([Arg.float, Arg.float, Arg.int], kernel="loan")
    # each of these reaches ONLY the new factor==1.0 branch: months>0 (skips the months<=0 guard),
    # r != 0.0 (skips the r==0 branch), yet factor ends exactly 1.0
    cases = [
        {"key": "round1", "handler": h_loan, "inputs": [1000.0, 1e-300, 12]},   # 1.0 + r rounds to 1.0
        {"key": "cycle", "handler": h_loan, "inputs": [1000.0, -2400.0, 2]},    # (-1)^2 == 1.0
        {"key": "norm", "handler": h_loan, "inputs": [1000.0, 12.0, 12]},       # the control: a real payment
    ]
    got = _node_run(cs, mod, cases, tmp_path)
    for key, args in (("round1", (1000.0, 1e-300, 12)), ("cycle", (1000.0, -2400.0, 2))):
        want = float_bits(b.run_python(*args))
        assert got[key]["bits"] == want and got[key]["lastError"] is None, f"{key}: WASM must agree with CPython (both principal/months), got {got[key]}"
        assert got[key]["value"] == args[0] / args[2], f"{key}: the guard returns principal/months"
        # sanity: without the guard this input is a zero-denominator (the fix is load-bearing)
        assert b.run_python(*args) != b.run_python(args[0], 12.0, args[2]), f"{key}: the guarded value differs from the normal-rate formula"
    assert got["norm"]["bits"] != got["round1"]["bits"], "the normal-rate control must diverge (equality is discriminating)"


def test_node_client_refusals_are_surfaced_never_coerced(cs, mod, tmp_path):
    """Every input refusal is a loud 'error (in-tab @wasm, NO fallback)' in the output (and
    `lastError`), never a silent coercion: `Arg.int` refuses 12.5 and "12x", a mis-wired handler
    (wrong input count) is refused, an unknown kernel too. PAIRED: the same handlers with GOOD
    inputs succeed in the same run (the refusals are specific)."""
    h_loan = cs.call_js([Arg.float, Arg.float, Arg.int], kernel="loan", status=True)
    h_loan_one = cs.call_js([Arg.const(1000.0), Arg.const(5.0), Arg.int], kernel="loan")
    h_ints = cs.call_js([Arg.js("v => v", produces="list[int]")], kernel="luhn")
    cases = [
        {"key": "trunc", "handler": h_loan, "inputs": [1000.0, 5.0, 12.5]},
        {"key": "notnum", "handler": h_loan, "inputs": [1000.0, "5%", 12]},
        {"key": "empty", "handler": h_loan, "inputs": ["", 5.0, 12]},
        # blocker 2: silent JS coercions the `num` guard must REFUSE, not accept --
        # Number([])===0 (an empty multiselect), Number("0x10")===16 (a hex string) would
        # otherwise become a legitimate-looking 0 / 16 and corrupt the call with no error.
        {"key": "arr", "handler": h_loan, "inputs": [[], 5.0, 12]},
        {"key": "hex", "handler": h_loan, "inputs": [1000.0, "0x10", 12]},
        {"key": "hexint", "handler": h_loan, "inputs": [1000.0, 5.0, "0x10"]},
        # the full non-decimal / non-number matrix the `num` guard must REFUSE (never coerce):
        {"key": "booltrue", "handler": h_loan, "inputs": [True, 5.0, 12]},    # Number(true)===1
        {"key": "boolfalse", "handler": h_loan, "inputs": [False, 5.0, 12]},  # Number(false)===0
        {"key": "null", "handler": h_loan, "inputs": [None, 5.0, 12]},        # Number(null)===0
        {"key": "comma", "handler": h_loan, "inputs": ["1,2", 5.0, 12]},
        {"key": "spaces", "handler": h_loan, "inputs": ["   ", 5.0, 12]},     # Number("   ")===0
        {"key": "incexp", "handler": h_loan, "inputs": ["1e", 5.0, 12]},
        {"key": "inf", "handler": h_loan, "inputs": ["Infinity", 5.0, 12]},   # Number("Infinity")===Infinity
        {"key": "nan", "handler": h_loan, "inputs": ["NaN", 5.0, 12]},
        {"key": "unsafeint", "handler": h_loan, "inputs": [1000.0, 5.0, 2 ** 53]},  # scalar Arg.int safe-integer boundary
        {"key": "count", "handler": h_loan, "inputs": [1000.0, 5.0]},
        {"key": "count1", "handler": h_loan_one, "inputs": [12, 13]},
        {"key": "str12", "handler": h_loan_one, "inputs": ["12"]},
        {"key": "good", "handler": h_loan, "inputs": [1000.0, 5.0, 12]},
        # legitimate decimal-shaped Textbox strings the guard must ACCEPT (== the numeric twin):
        {"key": "gexp", "handler": h_loan, "inputs": [1000.0, "1e1", 12]},    # "1e1" == 10.0
        {"key": "gexp_n", "handler": h_loan, "inputs": [1000.0, 10.0, 12]},
        {"key": "gspace", "handler": h_loan, "inputs": [1000.0, " 5 ", 12]},  # " 5 " == 5.0 (trimmed)
        {"key": "gneg", "handler": h_loan, "inputs": ["-3.5", 5.0, 12]},      # signed decimal
        {"key": "gneg_n", "handler": h_loan, "inputs": [-3.5, 5.0, 12]},
        {"key": "i64", "handler": h_ints, "inputs": [[1.5]]},
        {"key": "goodints", "handler": h_ints, "inputs": [[4, 2, 4, 2]]},
    ]
    got = _node_run(cs, mod, cases, tmp_path)
    assert "expected an integer" in got["trunc"]["result"][0] and got["trunc"]["result"][0].startswith("error (in-tab @wasm, NO fallback): ")
    assert "parameter `months` (int)" in got["trunc"]["lastError"]
    assert "expected a finite number" in got["notnum"]["result"][0] and "expected a finite number" in got["empty"]["result"][0]
    # blocker 2: [] and "0x10" are refused loudly, NOT coerced to 0 / 16
    assert "expected a finite number" in got["arr"]["result"][0] and got["arr"]["result"][0].startswith("error (")
    assert "expected a finite number" in got["hex"]["result"][0] and got["hex"]["result"][0].startswith("error (")
    assert "expected an integer" in got["hexint"]["result"][0] and got["hexint"]["result"][0].startswith("error (")
    # SF2: every non-decimal / non-number form is refused (bool/null/hex/comma/space/inf/nan/incomplete-exp)
    for k in ("booltrue", "boolfalse", "null", "comma", "spaces", "incexp", "inf", "nan"):
        assert "expected a finite number" in got[k]["result"][0] and got[k]["result"][0].startswith("error ("), f"{k} must be refused, not coerced"
    assert "expected an integer" in got["unsafeint"]["result"][0] and "2**53" in got["unsafeint"]["result"][0]  # the scalar safe-integer bound
    assert "expects 3 Gradio input(s)" in got["count"]["result"][0] and "expects 1 Gradio input(s)" in got["count1"]["result"]
    assert got["str12"]["value"] == got["good"]["value"], "a numeric string for Arg.int is admitted (a Textbox '12')"
    assert isinstance(got["good"]["result"][0], (int, float)) and got["good"]["lastError"] is None  # the raw Number, not an error string
    # SF2: legitimate decimal-shaped strings are accepted and equal their numeric twin
    assert got["gexp"]["value"] == got["gexp_n"]["value"] and got["gexp"]["lastError"] is None, "'1e1' == 10.0"
    assert got["gspace"]["value"] == got["good"]["value"] and got["gspace"]["lastError"] is None, "' 5 ' == 5.0 (trimmed)"
    assert got["gneg"]["value"] == got["gneg_n"]["value"] and got["gneg"]["lastError"] is None, "'-3.5' == -3.5"
    assert "not a safe integer" in got["i64"]["result"] and got["i64"]["result"].startswith("error (")  # the shim's own refusal, surfaced
    assert got["goodints"]["value"] == 0.0 and got["goodints"]["lastError"] is None


def test_node_two_client_side_hooks_compose_and_a_conflicting_name_is_refused(cs, mod, tmp_path):
    """Two `client_side(...)` objects on one page (one global): the second hook MERGES its kernels
    into the live client (the first's handlers keep working); a display name already bound to a
    DIFFERENT kernel is refused (thrown -> a load error), never silently overridden."""
    from pythscribe import binding_of

    node = gate_node()
    a = client_side({"luhn": mod.luhn_ok})
    b = client_side({"loan": mod.monthly_payment})
    c = client_side({"luhn": mod.pii_scan})  # the SAME display name, a different kernel

    def spec_for(x):
        return {k: dict(v, wasm=binding_of(getattr(mod, v["fn"])).artifact.wasm.resolve().as_posix()) for k, v in x.spec.items()}

    runner = tmp_path / "compose.mjs"
    runner.write_text(
        f"import * as ffi from {json.dumps(SHIM.resolve().as_uri())};\nconst resolveUrl = (u) => u;\n{SCALAR_CLIENT_JS.read_text(encoding='utf-8')}\n"
        "globalThis.window = {};\n"
        f"window.g = makeScalarClient(ffi, mergeScalarSpec(window.g, {json.dumps(spec_for(a))})); window.g.ready = true;\n"
        f"window.g = makeScalarClient(ffi, mergeScalarSpec(window.g, {json.dumps(spec_for(b))})); window.g.ready = true;\n"
        "const r1 = await window.g.invoke('luhn', ['4242 4242 4242 4242'], [{t: 'digits'}], null, false);\n"
        "const r2 = await window.g.invoke('loan', [1000, 0, 10], [{t: 'float'}, {t: 'float'}, {t: 'int'}], null, false);\n"
        "let err = null; try { mergeScalarSpec(window.g, " + json.dumps(spec_for(c)) + "); } catch (e) { err = e.message; }\n"
        "console.log(JSON.stringify({ kernels: Object.keys(window.g.spec), r1, r2, err }));\n",
        encoding="utf-8")
    p = subprocess.run([node, str(runner)], capture_output=True, text=True, encoding="utf-8", timeout=120)
    assert p.returncode == 0, p.stderr[-3000:]
    out = json.loads(p.stdout.strip().splitlines()[-1])
    assert out["kernels"] == ["luhn", "loan"] and out["r1"] == 0.0 and out["r2"] == 100.0
    assert "already bound to a different kernel" in out["err"] and "luhn_ok" in out["err"] and "pii_scan" in out["err"]
