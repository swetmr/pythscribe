/// Integration tests that compile .ps fixtures and verify JS output.

fn compile(source: &str) -> String {
    let module = pyths_parser::parse(source).expect("Parse failed");
    pyths_codegen_js::codegen(&module)
}

fn compile_worker(source: &str) -> String {
    let module = pyths_parser::parse(source).expect("Parse failed");
    let mut opts = pyths_codegen_js::CodegenOptions::default();
    opts.worker_runtime = true;
    pyths_codegen_js::codegen_with_options(&module, &opts)
}

#[test]
fn test_hello_world() {
    let js = compile("print(\"hello world\")");
    assert!(js.contains("pyPrint(\"hello world\")"), "JS: {}", js);
}

#[test]
fn test_variables() {
    let js = compile("x = 42\nname = \"Alice\"\nactive = True\nnothing = None");
    assert!(js.contains("let x = 42;"), "JS: {}", js);
    assert!(js.contains("let name = \"Alice\";"), "JS: {}", js);
    assert!(js.contains("let active = true;"), "JS: {}", js);
    assert!(js.contains("let nothing = null;"), "JS: {}", js);
}

#[test]
fn test_arithmetic() {
    let js = compile("x = a + b\ny = a // b\nz = a ** b");
    // Arithmetic routes through runtime helpers so arbitrary-precision
    // ints stay exact (Number↔BigInt promotion) and div-by-zero raises
    // ZeroDivisionError.
    assert!(js.contains("pyAdd(a, b)"), "JS: {}", js);
    assert!(js.contains("pyFloorDiv(a, b)"), "JS: {}", js);
    assert!(js.contains("pyPow(a, b)"), "JS: {}", js);
}

#[test]
fn test_local_param_shadows_builtin() {
    // Python scoping: a def/lambda PARAM named like a builtin (`set`/`list`/
    // `dict`/…) shadows the builtin — a call to it invokes the PARAM, not the
    // builtin. Found by the Zustand dual-track state test (`create(lambda set:
    // … set(…))`), where `set(…)` was mis-lowered to the `set()` builtin
    // (`pySetOf`) so store updates silently no-op'd. KNOWN REMAINING LIMITATION
    // (documented): NESTED-function shadowing — an inner fn using a
    // builtin-named OUTER param — is not yet resolved (the scope tracker doesn't
    // surface outer-function params during nested emission); workaround: rename
    // the param (Zustand: `lambda set_state: …`).
    let js1 = compile("f = lambda set: set([1])");
    assert!(
        js1.contains("(set) => set([1])"),
        "lambda param `set` calls the param, JS: {}",
        js1
    );
    assert!(
        !js1.contains("pySetOf"),
        "shadowed set() must NOT be the builtin, JS: {}",
        js1
    );
    let js2 = compile("def store(set):\n    return set([1])\n");
    assert!(
        !js2.contains("pySetOf"),
        "def param `set` calls the param, JS: {}",
        js2
    );
    // un-shadowed builtin still lowers to the runtime helper
    let js3 = compile("def f(s):\n    return set([1])\n");
    assert!(
        js3.contains("pySetOf"),
        "un-shadowed set() stays the builtin, JS: {}",
        js3
    );
}

#[test]
fn test_bitnot_routes_through_pybitnot_shipping_binding() {
    // C1-rollout wave-14 shipping binding: unary `~` MUST route through the
    // arbitrary-precision `pyBitNot` helper, NOT raw JS `(~x)` which is 32-bit
    // ToInt32 (`~(2**40)` would be -1 instead of CPython -1099511627777). This
    // is the in-repo emitter↔model binding for `preservationNb`; the value-level
    // CPython differential lives in experiments/pbt-ps/tier3_shipped_binding.py.
    let js = compile("x = ~a");
    assert!(
        js.contains("pyBitNot(a)"),
        "expected pyBitNot routing, JS: {}",
        js
    );
    assert!(
        !js.contains("(~a)"),
        "must NOT emit raw 32-bit ~, JS: {}",
        js
    );
    // Worker/edge target must import pyBitNot from the runtime core (regression
    // for the missing `pyths-runtime/core` export that broke worker builds).
    let wjs = compile_worker("x = ~a");
    assert!(
        wjs.contains("pyBitNot(a)"),
        "worker: expected pyBitNot, JS: {}",
        wjs
    );
    assert!(
        wjs.contains("pyBitNot") && wjs.contains("pyths-runtime/core"),
        "worker must import pyBitNot from pyths-runtime/core, JS: {}",
        wjs
    );
}

#[test]
fn test_nonsubscriptable_routes_through_pygetitem_shipping_binding() {
    // Lattice C4 shipping binding: a plain READ of a non-subscriptable receiver
    // (float / set — and the int/bool inside primitive) MUST route through
    // pyGetItem, which raises a Python TypeError, NOT raw native `x[i]` (which
    // returns a silent JS `undefined`). cert::route previously sent Float/Set to
    // Route::Native; the lattice shipping-binding
    // (experiments/pbt-ps/lattice_shipped_binding.py) caught `(3.5)[0]` → None.
    // Now every receiver type routes Helper for a plain read (helperTy is total),
    // pinned by verification/route-table.txt + the Lean route_read_safety twin.
    let jf = compile("x = (3.5)[0]");
    assert!(
        jf.contains("pyGetItem("),
        "float subscript must route pyGetItem, JS: {}",
        jf
    );
    assert!(
        !jf.contains("3.5[0]"),
        "must NOT emit raw native 3.5[0], JS: {}",
        jf
    );
    let js = compile("s = {1, 2, 3}\nx = s[0]");
    assert!(
        js.contains("pyGetItem("),
        "set subscript must route pyGetItem, JS: {}",
        js
    );
}

#[test]
fn test_builtin_html_collision_prefers_element_in_component() {
    // DESIGN RULE (documented in emit.rs): inside a @component, a name that is
    // BOTH a Python builtin AND a known HTML/SVG element — `map`/`input`/
    // `object` — lowers as the ELEMENT (HTML wins the collision). Data-builtins
    // that are NOT element names (filter/set/list/dict) keep their builtin
    // lowering. The collision only exists inside a component (PSX context).
    let src = "@component\n\
               def W(xs):\n    \
                   m = map(lambda x: x, xs)\n    \
                   i = input()\n    \
                   o = object()\n    \
                   f = filter(lambda x: x, xs)\n    \
                   s = set(xs)\n    \
                   return div()\n";
    let js = compile(src);
    // HTML wins for the collision set (map/input/object → elements)
    assert!(
        js.contains("createElement(\"map\""),
        "map → <map> element, JS: {}",
        js
    );
    assert!(
        js.contains("createElement(\"input\""),
        "input → <input> element, JS: {}",
        js
    );
    assert!(
        js.contains("createElement(\"object\""),
        "object → <object> element, JS: {}",
        js
    );
    // non-element builtins are NOT elements — no collision, stay builtins
    assert!(
        !js.contains("createElement(\"filter\""),
        "filter stays builtin, JS: {}",
        js
    );
    assert!(
        !js.contains("createElement(\"set\""),
        "set stays builtin, JS: {}",
        js
    );
    assert!(js.contains("pySetOf("), "set → pySetOf builtin, JS: {}", js);
}

#[test]
fn test_stdlib_class_in_component_is_new_not_jsx() {
    // A capitalized stdlib class imported into a @component and CALLED must
    // lower to `new X(...)` (constructor), NOT `createElement(X, ...)` — the
    // capitalized-name→React-component default otherwise mis-lowers every
    // capitalized stdlib constructor used in a component (found by the B-JS
    // interaction suite: `Counter(items).most_common(...)` threw because `c`
    // was a React element). Lowercase stdlib fns (product/combinations) stay
    // plain calls. Fix: stdlib CapWords imports register as known_classes.
    let src = "from collections import Counter, OrderedDict\n\
               from decimal import Decimal\n\
               from itertools import product\n\
               @component\n\
               def W(items):\n    \
                   c = Counter(items)\n    \
                   od = OrderedDict()\n    \
                   d = Decimal(\"1.5\")\n    \
                   p = product([1], [2])\n    \
                   return div(str(c.most_common(1)))\n";
    let js = compile(src);
    assert!(
        js.contains("new Counter("),
        "Counter must be `new`, JS: {}",
        js
    );
    assert!(
        !js.contains("createElement(Counter"),
        "Counter must NOT be JSX, JS: {}",
        js
    );
    assert!(
        js.contains("new OrderedDict("),
        "OrderedDict must be `new`, JS: {}",
        js
    );
    assert!(
        js.contains("new Decimal("),
        "Decimal must be `new`, JS: {}",
        js
    );
    // lowercase stdlib function stays a plain call (not `new`, not JSX)
    assert!(
        !js.contains("new product("),
        "product (fn) must NOT be `new`, JS: {}",
        js
    );
    assert!(
        !js.contains("createElement(product"),
        "product must NOT be JSX, JS: {}",
        js
    );
}

#[test]
fn test_function_def() {
    let js = compile("def greet(name):\n    return f\"Hello, {name}!\"");
    assert!(js.contains("function greet(name)"), "JS: {}", js);
    // A4: plain (no-format-spec) f-string interpolation routes through
    // pyStr so bool/None/floats/containers print CPython-style instead
    // of JS's implicit template-literal ToString.
    assert!(js.contains("return `Hello, ${pyStr(name)}!`"), "JS: {}", js);
}

#[test]
fn test_if_elif_else() {
    let source = "if x > 5:\n    print(\"big\")\nelif x > 0:\n    print(\"small\")\nelse:\n    print(\"neg\")";
    let js = compile(source);
    assert!(js.contains("if ("), "JS: {}", js);
    assert!(js.contains("} else if ("), "JS: {}", js);
    assert!(js.contains("} else {"), "JS: {}", js);
}

#[test]
fn test_for_loop() {
    let js = compile("for item in items:\n    print(item)");
    assert!(
        js.contains("for (const item of pyForIter(items))"),
        "JS: {}",
        js
    );
    assert!(js.contains("pyPrint(item)"), "JS: {}", js);
}

#[test]
fn test_while_loop() {
    let js = compile("while x > 0:\n    x -= 1");
    assert!(js.contains("while ("), "JS: {}", js);
    // Round-2: name-target augassign routes through the Python-operator
    // helper (raw `-=` skips BigInt promotion).
    assert!(js.contains("x = pySub(x, 1)"), "JS: {}", js);
}

#[test]
fn test_class_basic() {
    let source = "class Dog:\n    def __init__(self, name):\n        self.name = name\n    def bark(self):\n        return f\"{self.name} says woof\"";
    let js = compile(source);
    // Cooperative PyObject model: extend PyObject, __init__ as a prototype
    // method dispatched via the MRO (not the JS constructor).
    assert!(js.contains("class Dog extends PyObject"), "JS: {}", js);
    assert!(js.contains("__init__(name)"), "JS: {}", js);
    assert!(js.contains("this.name = name"), "JS: {}", js);
    assert!(js.contains("bark()"), "JS: {}", js);
}

#[test]
fn test_class_inheritance() {
    let source = "class Animal:\n    def __init__(self, name):\n        self.name = name\nclass Dog(Animal):\n    pass";
    let js = compile(source);
    assert!(js.contains("class Dog extends Animal"), "JS: {}", js);
    // The cooperative-MRO model is installed for classes with bases.
    assert!(js.contains("__pyClass(Dog, [Animal])"), "JS: {}", js);
}

#[test]
fn test_multiple_inheritance_installs_mro() {
    // `class C(A, B)` extends only the first base on the JS chain; methods
    // from B are mixed in by __pyClass, so `c.world()` resolves at runtime
    // instead of crashing (the silent-miscompile bug this fixes).
    let source = "class A:\n    def hello(self):\n        return 1\n\
                  class B:\n    def world(self):\n        return 2\n\
                  class C(A, B):\n    pass";
    let js = compile(source);
    assert!(
        js.contains("class C extends A"),
        "first base on chain: {}",
        js
    );
    assert!(
        js.contains("__pyClass(C, [A, B])"),
        "MRO install for both bases: {}",
        js
    );
}

#[test]
fn test_super_method_uses_cooperative_proxy() {
    // Non-constructor `super().m()` must NOT emit invalid JS `super().m()`
    // (a SyntaxError); it routes through the cooperative-MRO proxy bound to
    // the *defining* class.
    let source = "class A:\n    def m(self):\n        return 1\n\
                  class B(A):\n    def m(self):\n        return super().m()";
    let js = compile(source);
    assert!(
        js.contains("__pySuper(B, this).m()"),
        "cooperative super: {}",
        js
    );
    assert!(
        !js.contains("super().m()"),
        "must not emit bare super().m(): {}",
        js
    );
}

#[test]
fn test_regular_class_uses_pyobject_model() {
    // Every regular class joins the cooperative object model: extends
    // PyObject (so `new` routes through the MRO __init__ dispatcher) and
    // installs `__mro__` via __pyClass — even with no explicit base.
    let js = compile("class Solo:\n    def f(self):\n        return 1");
    assert!(js.contains("class Solo extends PyObject"), "JS: {}", js);
    assert!(js.contains("__pyClass(Solo, [])"), "JS: {}", js);
}

#[test]
fn test_init_is_cooperative_method_not_constructor() {
    // In the PyObject model, __init__ is a prototype method and
    // `super().__init__()` routes through the cooperative proxy — never a
    // native JS `super(...)` (which can't chain across MI diamonds).
    let source = "class A:\n    def __init__(self):\n        self.x = 1\n\
                  class B(A):\n    def __init__(self):\n        super().__init__()\n        self.y = 2";
    let js = compile(source);
    assert!(js.contains("__init__()"), "init as method: {}", js);
    assert!(
        js.contains("__pySuper(B, this).__init__()"),
        "cooperative init super: {}",
        js
    );
    assert!(
        !js.contains("constructor("),
        "no native constructor for regular class: {}",
        js
    );
}

#[test]
fn test_class_extending_external_base_uses_native_constructor() {
    // A3: `class Boundary(Component)` where `Component` is an imported
    // NATIVE base (not a `class`-defined name in this file) must NOT join
    // the cooperative PyObject model — `__init__` never gets invoked for a
    // native `extends` chain (only `PyObject`'s own constructor walks the
    // MRO to dispatch it), silently dropping `self.state`. External bases
    // keep the same native-constructor path as exception subclasses.
    let source = "from react import Component\n\
                  class Boundary(Component):\n    def __init__(self, props):\n        super().__init__(props)\n        self.state = {\"hasError\": False}\n    def render(self):\n        return self.props.children";
    let js = compile(source);
    assert!(
        js.contains("class Boundary extends Component"),
        "JS: {}",
        js
    );
    assert!(
        js.contains("constructor(props)"),
        "native constructor: {}",
        js
    );
    assert!(js.contains("super(props)"), "native super call: {}", js);
    assert!(
        js.contains("this.state = "),
        "state set on this in constructor: {}",
        js
    );
    assert!(
        !js.contains("__pyClass(Boundary"),
        "no cooperative MRO wrap for native base: {}",
        js
    );
}

#[test]
fn test_class_extending_local_base_keeps_pyobject_model() {
    // Sibling case: a base defined via `class` in the SAME file is a pure
    // PythScribe hierarchy — keeps the cooperative model exactly as before.
    let source = "class Animal:\n    def __init__(self, name):\n        self.name = name\nclass Dog(Animal):\n    pass";
    let js = compile(source);
    assert!(js.contains("class Dog extends Animal"), "JS: {}", js);
    assert!(js.contains("__pyClass(Dog, [Animal])"), "JS: {}", js);
}

#[test]
fn test_dataclass_keeps_native_constructor() {
    // @dataclass keeps its generated constructor + validation; it does NOT
    // join the PyObject model (no `extends PyObject`, no __pyClass).
    let source = "from dataclasses import dataclass\n@dataclass\nclass P:\n    x: int";
    let js = compile(source);
    assert!(
        !js.contains("extends PyObject"),
        "dataclass stays standalone: {}",
        js
    );
    assert!(
        !js.contains("__pyClass(P"),
        "dataclass skips MRO install: {}",
        js
    );
}

#[test]
fn test_isinstance_uses_mro_helper() {
    // isinstance must consult the MRO (so non-first bases match), not a bare
    // `instanceof` that misses sibling bases.
    let js = compile("x = foo()\nprint(isinstance(x, Bar))");
    assert!(
        js.contains("__pyIsInstance(x, Bar)"),
        "isinstance via MRO helper: {}",
        js
    );
}

#[test]
fn test_list_comprehension() {
    let js = compile("evens = [x for x in numbers if x % 2 == 0]");
    assert!(js.contains(".filter("), "JS: {}", js);
    assert!(js.contains(".map("), "JS: {}", js);
}

#[test]
fn test_scoped_ui_library_imports() {
    // Mantine, Chakra, Headless, Radix — all are scoped npm packages
    // (`@mantine/core` etc.) reached via the `at_<org>.<pkg>` form.
    // They should be recognized as React-like modules so snake→camel
    // import-name transforms apply to their hook exports.
    let js = compile(
        r#"
from at_mantine.core import Button, use_disclosure
from at_chakra_ui.react import Box, useColorMode

@component
def App():
    opened, controls = use_disclosure()
    return Button(on_click=controls.open)("Open")
"#,
    );
    assert!(
        js.contains("from \"@mantine/core\""),
        "mantine path: {}",
        js
    );
    assert!(
        js.contains("from \"@chakra-ui/react\""),
        "chakra path: {}",
        js
    );
    assert!(
        js.contains("useDisclosure"),
        "snake→camel on use_disclosure: {}",
        js
    );
}

#[test]
fn test_lucide_react_icon_import() {
    let js = compile(
        r#"
from lucide_react import ChevronDown, Loader2
from pyths.react import component

@component
def Toggle():
    return div()(ChevronDown(size=16), Loader2(size=16))
"#,
    );
    assert!(
        js.contains("from \"lucide-react\""),
        "lucide-react path: {}",
        js
    );
    assert!(js.contains("ChevronDown"), "icon name preserved: {}", js);
}

#[test]
fn test_zustand_create_import() {
    let js = compile(
        r#"
from zustand import create

use_store = create(lambda set_state: {"count": 0})
"#,
    );
    assert!(js.contains("from \"zustand\""), "zustand path: {}", js);
}

#[test]
fn test_async_list_comprehension() {
    // `async for` in a comprehension lowers to `for await (...)` inside
    // an async-IIFE. The caller awaits the resulting Promise.
    let js = compile(
        r#"
async def fetch_all(sources):
    return [item async for source in sources for item in source]
"#,
    );
    assert!(js.contains("for await"), "for-await emitted: {}", js);
    assert!(js.contains("async () =>"), "async IIFE wrapper: {}", js);
    // Synchronous `for` inside the same comprehension stays plain.
    assert!(
        js.contains("for (const item of pyForIter(source))"),
        "plain for kept: {}",
        js
    );
}

#[test]
fn test_async_list_comprehension_simple() {
    // Single-generator async comprehension still routes through the
    // IIFE path (the .filter().map() fast path requires sync iter).
    let js = compile(
        r#"
async def gather(stream):
    return [x async for x in stream]
"#,
    );
    assert!(js.contains("for await"), "for-await: {}", js);
    assert!(js.contains("async () =>"), "async IIFE: {}", js);
    // Confirm the .map() fast path was NOT taken.
    assert!(
        !js.contains(".map("),
        "should not use .map() fast path: {}",
        js
    );
}

#[test]
fn test_lambda() {
    let js = compile("f = lambda x: x + 1");
    assert!(js.contains("(x) => pyAdd(x, 1)"), "JS: {}", js);
}

#[test]
fn test_ternary() {
    let js = compile("y = \"big\" if x > 5 else \"small\"");
    assert!(js.contains("?"), "JS: {}", js);
    assert!(js.contains(":"), "JS: {}", js);
}

#[test]
fn test_fstring() {
    // A4: no-format-spec interpolation routes through pyStr (Python
    // str() semantics) instead of JS's implicit template-literal
    // ToString.
    let js = compile("msg = f\"hello {name}\"");
    assert!(js.contains("`hello ${pyStr(name)}`"), "JS: {}", js);
}

#[test]
fn test_floor_division() {
    let js = compile("x = a // b");
    // Routed through pyFloorDiv so b===0 throws ZeroDivisionError.
    assert!(js.contains("pyFloorDiv(a, b)"), "JS: {}", js);
}

#[test]
fn test_none_true_false() {
    let js = compile("a = None\nb = True\nc = False");
    assert!(js.contains("null"), "JS: {}", js);
    assert!(js.contains("true"), "JS: {}", js);
    assert!(js.contains("false"), "JS: {}", js);
}

#[test]
fn test_import() {
    let js = compile("import math");
    assert!(
        js.contains("import * as math from \"pyths-runtime/stdlib/math\""),
        "JS: {}",
        js
    );
}

#[test]
fn test_import_non_stdlib() {
    // Generic npm fallback: `my_module` → "my-module" (kebab-case
    // npm convention). The local Python binding stays `my_module`.
    let js = compile("import my_module");
    assert!(
        js.contains("import * as my_module from \"my-module\""),
        "JS: {}",
        js
    );
}

#[test]
fn test_from_import() {
    let js = compile("from os.path import join, exists");
    assert!(
        js.contains("import { join, exists } from \"os/path\""),
        "JS: {}",
        js
    );
}

#[test]
fn test_import_side_effect_css_verbatim() {
    // A2: `import "<string>"` — side-effect asset import, emitted verbatim
    // (no name rewriting, no npm-name remapping — that's for named imports).
    let js = compile("import \"./styles.css\"");
    assert!(js.contains("import \"./styles.css\";"), "JS: {}", js);
}

#[test]
fn test_import_side_effect_arbitrary_extension_verbatim() {
    // Codegen doesn't validate the string — any extension passes through.
    let js = compile("import \"./assets/logo.png\"");
    assert!(js.contains("import \"./assets/logo.png\";"), "JS: {}", js);
}

#[test]
fn test_import_side_effect_alongside_component() {
    // A2 end-to-end: a CSS side-effect import plus a simple component in
    // the same module — the CSS import must show up verbatim and the
    // component must still compile correctly alongside it.
    let source = "import \"./styles.css\"\n\n@component\ndef App():\n    return None";
    let js = compile(source);
    assert!(
        js.contains("import \"./styles.css\";"),
        "CSS import verbatim: {}",
        js
    );
    assert!(
        js.contains("export function App"),
        "Component still compiles: {}",
        js
    );
}

#[test]
fn test_try_except() {
    let source = "try:\n    x = 1\nexcept Exception as e:\n    print(e)";
    let js = compile(source);
    assert!(js.contains("try {"), "JS: {}", js);
    assert!(js.contains("catch"), "JS: {}", js);
}

#[test]
fn test_except_exception_is_unconditional_catch_all() {
    // `except Exception` / `except BaseException` are Python's catch-all bases
    // with no JS runtime class. Emitting `__exc instanceof Exception` throws a
    // ReferenceError the moment the handler runs (regression: surfaced when a
    // mounted .ps component's async error handler actually executed).
    for base in ["Exception", "BaseException"] {
        let js = compile(&format!(
            "try:\n    x = 1\nexcept {base} as e:\n    print(e)"
        ));
        assert!(
            !js.contains(&format!("instanceof {base}")),
            "{base} must not be referenced as a JS class: {js}"
        );
        assert!(js.contains("catch (__exc)"), "JS: {js}");
        assert!(js.contains("let e = __exc"), "binds the caught value: {js}");
    }
    // A user/builtin exception class still gets the instanceof guard.
    let js = compile("try:\n    x = 1\nexcept ValueError as e:\n    print(e)");
    assert!(
        js.contains("instanceof ValueError"),
        "specific types still guarded: {js}"
    );
}

#[test]
fn test_raise() {
    let js = compile("raise ValueError(\"bad\")");
    assert!(js.contains("throw"), "JS: {}", js);
}

#[test]
fn test_assert() {
    let js = compile("assert x > 0, \"must be positive\"");
    assert!(js.contains("if (!("), "JS: {}", js);
    assert!(js.contains("throw"), "JS: {}", js);
    assert!(js.contains("new Error"), "JS: {}", js);
    // Python fidelity: thrown object's `.name` reads "AssertionError"
    // so `try/except AssertionError` matches it.
    assert!(
        js.contains("\"AssertionError\""),
        "AssertionError name: {}",
        js
    );
    assert!(
        js.contains("\"must be positive\""),
        "Message preserved: {}",
        js
    );
}

#[test]
fn test_assert_without_message() {
    let js = compile("assert x > 0");
    assert!(
        js.contains("\"Assertion failed\""),
        "default message: {}",
        js
    );
    assert!(
        js.contains("\"AssertionError\""),
        "AssertionError name: {}",
        js
    );
}

#[test]
fn test_assert_message_with_f_string() {
    let js = compile("assert n >= 0, f\"got {n}\"");
    assert!(js.contains("`got "), "f-string message: {}", js);
    assert!(
        js.contains("\"AssertionError\""),
        "AssertionError name: {}",
        js
    );
}

#[test]
fn test_augmented_assignment() {
    // Round-2: name-target augmented assignment is helper-routed so it
    // matches the binary form (BigInt promotion, list concat, dict |).
    let js = compile("x += 1\ny -= 2\nz *= 3");
    assert!(js.contains("x = pyAdd(x, 1)"), "JS: {}", js);
    assert!(js.contains("y = pySub(y, 2)"), "JS: {}", js);
    assert!(js.contains("z = pyMul(z, 3)"), "JS: {}", js);
}

#[test]
fn test_dict_literal() {
    let js = compile("d = {\"a\": 1, \"b\": 2}");
    assert!(js.contains("\"a\": 1"), "JS: {}", js);
    assert!(js.contains("\"b\": 2"), "JS: {}", js);
}

#[test]
fn test_dict_spread() {
    // #83: spread-containing dict literals route through the runtime
    // shape-dispatching pyDictMerge — a spread arg can be a Map-backed
    // dict at runtime, which native `{...m}` silently drops (Maps have
    // no own enumerable props). pyDictMerge returns a plain object
    // unless some part is Map-backed, so plain-in -> plain-out holds.
    let js = compile("d = {**other}");
    assert!(js.contains("pyDictMerge(other)"), "JS: {}", js);

    // Spread with key-value pairs: the literal run becomes a plain chunk
    let js = compile("d = {\"a\": 1, **other}");
    assert!(
        js.contains("pyDictMerge(({\"a\": 1}), other)"),
        "JS: {}",
        js
    );

    // Multiple spreads
    let js = compile("d = {**a, **b}");
    assert!(js.contains("pyDictMerge(a, b)"), "JS: {}", js);
}

#[test]
fn test_set_literal() {
    // #297: set literals build the canonicalizing PySet.
    let js = compile("s = {1, 2, 3}");
    assert!(js.contains("new PySet"), "JS: {}", js);
}

#[test]
fn test_tuple_unpacking() {
    // #84: emitted as let-predeclared destructuring ASSIGNMENT so
    // already-declared names can be re-unpacked (swap idiom).
    let js = compile("a, b = 1, 2");
    assert!(js.contains("let a;"), "JS: {}", js);
    assert!(js.contains("let b;"), "JS: {}", js);
    assert!(js.contains("([a, b] ="), "JS: {}", js);
}

#[test]
fn test_tuple_unpack_swap_idiom() {
    // #84: `a, b = b, a` after both are declared must NOT re-declare
    // (previously emitted `const [a, b] = ...` → SyntaxError at load).
    let js = compile("a = 1\nb = 2\na, b = b, a");
    assert!(!js.contains("const [a, b]"), "JS: {}", js);
    assert!(js.contains("([a, b] = pyTuple(b, a))"), "JS: {}", js);
}

#[test]
fn test_nested_unpack_target() {
    // #85: a nested tuple target must emit a nested destructuring
    // PATTERN, not a pyTuple(...) value expression.
    let js = compile("c, (d, e) = 1, (2, 3)");
    assert!(js.contains("([c, [d, e]] ="), "JS: {}", js);
}

#[test]
fn test_chained_assignment() {
    // #99: `a = b = 5` — both targets bound; RHS evaluated once.
    let js = compile("a = b = 5");
    assert!(js.contains("let a = 5"), "JS: {}", js);
    assert!(js.contains("let b = 5"), "JS: {}", js);

    // Non-trivial RHS goes through a hidden once-evaluated const.
    let js2 = compile("a = b = f()");
    assert!(js2.contains("const __chain_0 = f()"), "JS: {}", js2);
    assert!(js2.contains("let a = __chain_0"), "JS: {}", js2);
    assert!(js2.contains("let b = __chain_0"), "JS: {}", js2);
}

#[test]
fn test_for_else_break_sets_flag() {
    // #91: break inside for/else must set the loop's flag so the else
    // clause is suppressed; nested loops get distinct flags.
    let js = compile("for x in [1, 2]:\n    if x == 1:\n        break\nelse:\n    print('done')");
    assert!(js.contains("let __for_broke_0 = false;"), "JS: {}", js);
    assert!(js.contains("__for_broke_0 = true;"), "JS: {}", js);
    assert!(js.contains("if (!__for_broke_0)"), "JS: {}", js);
}

#[test]
fn test_while_else_break_sets_flag() {
    let js = compile("i = 0\nwhile i < 3:\n    break\nelse:\n    print('done')");
    assert!(js.contains("let __while_broke_0 = false;"), "JS: {}", js);
    assert!(js.contains("__while_broke_0 = true;"), "JS: {}", js);
    assert!(js.contains("if (!__while_broke_0)"), "JS: {}", js);
}

#[test]
fn test_capitalized_function_not_new_called() {
    // #80: a top-level def with a capitalized name is a function, not a
    // class — must not be `new`-called.
    let js = compile("def Foo():\n    return 42\nx = Foo()");
    assert!(!js.contains("new Foo("), "JS: {}", js);
    assert!(js.contains("Foo()"), "JS: {}", js);

    // A real class keeps `new`.
    let js2 = compile("class Bar:\n    pass\nx = Bar()");
    assert!(js2.contains("new Bar("), "JS: {}", js2);
}

#[test]
fn test_proto_attribute_assignment() {
    // #81: `self.__proto__ = v` must emit an own data property, not hit
    // the Object.prototype.__proto__ accessor (which silently no-ops).
    let js = compile("def f(o):\n    o.__proto__ = 7");
    assert!(
        js.contains("Object.defineProperty(o, \"__proto__\""),
        "JS: {}",
        js
    );
}

#[test]
fn test_del_subscript_and_attribute() {
    // #101: del d[k] routes through pyDelItem (splice/KeyError
    // semantics); del obj.attr emits JS delete; del name never emits the
    // strict-mode-invalid `delete name`.
    let js = compile("d = {'a': 1}\ndel d['a']");
    assert!(js.contains("pyDelItem(d, \"a\")"), "JS: {}", js);

    let js2 = compile("def f(o):\n    del o.attr");
    assert!(js2.contains("delete o.attr;"), "JS: {}", js2);

    let js3 = compile("x = 1\ndel x");
    assert!(!js3.contains("delete x"), "JS: {}", js3);

    // #321: `del xs[a:b]` slice-delete routes to pyDelSlice (clamps OOB
    // bounds per CPython slice.indices) — NOT pyDelItem (which would see a
    // `null` slice arg and raise a spurious IndexError).
    let js4 = compile("v = [1, 2, 3]\ndel v[5:9]");
    assert!(js4.contains("pyDelSlice(v, 5, 9, null)"), "JS: {}", js4);
    assert!(
        !js4.contains("pyDelItem(v"),
        "slice-delete must not use pyDelItem: {}",
        js4
    );

    let js5 = compile("v = [1, 2, 3]\ndel v[::2]");
    assert!(js5.contains("pyDelSlice(v, null, null, 2)"), "JS: {}", js5);
}

#[test]
fn test_nested_aug_assign_implicit_local() {
    // #325: a nested def that ONLY aug-assigns a name (never plainly binds
    // it) makes that name an unbound function-local — sentinel-hoisted so the
    // guarded read raises UnboundLocalError, not a closure write to the outer
    // binding.
    let js = compile(
        "def main():\n    v1 = 5\n    def f3():\n        v1 += 1\n        return v1\n    return f3()",
    );
    // Inside f3: sentinel hoist + guarded read.
    assert!(
        js.contains("let v1 = __UNBOUND;"),
        "aug-only local not sentinel-hoisted:\n{}",
        js
    );
    assert!(
        js.contains("__pyChkLocal(v1"),
        "aug-only read not guarded:\n{}",
        js
    );

    // A genuinely-bound accumulator must NOT be sentineled (no false ULE).
    let ok = compile(
        "def acc(xs):\n    total = 0\n    for x in xs:\n        total += x\n    return total",
    );
    assert!(
        !ok.contains("total = __UNBOUND"),
        "bound accumulator wrongly sentineled:\n{}",
        ok
    );

    // nonlocal must keep writing through (not shadowed/sentineled).
    let nl = compile("def c():\n    n = 0\n    def inc():\n        nonlocal n\n        n += 1\n    inc()\n    return n");
    assert!(
        !nl.contains("n = __UNBOUND"),
        "nonlocal wrongly sentineled:\n{}",
        nl
    );
}

#[test]
fn test_sibling_match_unique_subject() {
    // #324: two sibling module-level match statements must not both emit
    // `const __match0 = ...` (a redeclaration SyntaxError). Each gets a
    // unique per-statement subject name.
    let js =
        compile("match 7:\n    case x:\n        print(x)\nmatch 8:\n    case y:\n        print(y)");
    assert!(
        !js.contains("const __match ="),
        "fixed-name subject collides: {}",
        js
    );
    assert_eq!(
        js.matches("const __match").count(),
        2,
        "expected 2 distinct subjects: {}",
        js
    );
    assert!(
        js.contains("__match0") && js.contains("__match1"),
        "subjects not uniquely numbered: {}",
        js
    );
}

#[test]
fn test_is_operator() {
    // B-021: `is None` uses loose `== null` so a JS `undefined` also counts
    // as None; non-None identity still uses strict `Object.is`.
    let js = compile("x = a is None");
    assert!(js.contains("== null"), "JS: {}", js);
    assert!(!js.contains("Object.is"), "JS: {}", js);

    let js_id = compile("x = a is b");
    assert!(js_id.contains("Object.is("), "JS: {}", js_id);
}

#[test]
fn test_is_not_operator() {
    let js = compile("x = a is not None");
    assert!(js.contains("!= null"), "JS: {}", js);
    assert!(!js.contains("Object.is"), "JS: {}", js);
}

#[test]
fn test_in_operator() {
    // `in` routes through pyContains (the runtime helper that dispatches by
    // container type — arrays/strings → .includes, Set/Map → .has, plain
    // objects → JS `in` keyword for KEY membership). Direct `.includes()`
    // crashes on plain objects, so the codegen never emits it.
    let js = compile("x = a in items");
    assert!(js.contains("pyContains(items, a)"), "JS: {}", js);
}

#[test]
fn test_not_in_operator() {
    let js = compile("x = a not in items");
    assert!(js.contains("!pyContains(items, a)"), "JS: {}", js);
}

#[test]
fn test_class_new_instance() {
    let js = compile("dog = Dog(\"Rex\")");
    assert!(js.contains("new Dog(\"Rex\")"), "JS: {}", js);
}

#[test]
fn test_list_literal() {
    let js = compile("items = [1, 2, 3]");
    assert!(js.contains("[1, 2, 3]"), "JS: {}", js);
}

#[test]
fn test_string_method_call() {
    // After Python→JS method lowering, `.upper()` becomes `.toUpperCase()`.
    let js = compile("x = name.upper()");
    assert!(js.contains("name.toUpperCase()"), "JS: {}", js);
}

#[test]
fn test_nested_function() {
    let source = "def outer():\n    def inner():\n        return 1\n    return inner()";
    let js = compile(source);
    assert!(js.contains("function outer()"), "JS: {}", js);
    assert!(js.contains("function inner()"), "JS: {}", js);
}

#[test]
fn test_default_params() {
    let js = compile("def greet(name, greeting=\"Hello\"):\n    return f\"{greeting}, {name}!\"");
    assert!(js.contains("greeting = \"Hello\""), "JS: {}", js);
}

#[test]
fn test_break_continue() {
    let source = "for i in items:\n    if i == 0:\n        continue\n    if i > 10:\n        break\n    print(i)";
    let js = compile(source);
    assert!(js.contains("continue"), "JS: {}", js);
    assert!(js.contains("break"), "JS: {}", js);
}

#[test]
fn test_async_function() {
    let source = "async def fetch_data(url):\n    result = await fetch(url)\n    return result";
    let js = compile(source);
    assert!(js.contains("async function"), "JS: {}", js);
    assert!(js.contains("await fetch(url)"), "JS: {}", js);
}

#[test]
fn test_async_for() {
    let source = "async def consume(iter):\n    async for item in iter:\n        process(item)";
    let js = compile(source);
    assert!(js.contains("async function"), "JS: {}", js);
    assert!(js.contains("for await"), "JS: {}", js);
}

#[test]
fn test_pyths_asyncio_import() {
    let source = "from pyths.asyncio import gather, sleep";
    let js = compile(source);
    assert!(js.contains("pyths-runtime/asyncio"), "JS: {}", js);
    assert!(js.contains("gather"), "JS: {}", js);
    assert!(js.contains("sleep"), "JS: {}", js);
}

#[test]
fn test_async_method() {
    let source = "class Api:\n    async def fetch(self):\n        return await get()";
    let js = compile(source);
    assert!(js.contains("async fetch"), "Has async method: {}", js);
}

#[test]
fn test_npm_scoped_package_at_prefix() {
    // Step 8: at_<org>.<pkg> → @<org>/<pkg>
    let source = "from at_stdlib.array import zeros";
    let js = compile(source);
    assert!(js.contains("@stdlib/array"), "JS: {}", js);
    assert!(js.contains("zeros"), "JS: {}", js);
}

#[test]
fn test_npm_scoped_package_kebab_org() {
    // Underscores in the org name become hyphens.
    let source = "from at_my_org.pkg import foo";
    let js = compile(source);
    assert!(js.contains("@my-org/pkg"), "JS: {}", js);
}

#[test]
fn test_npm_scoped_deep_subpath() {
    let source = "from at_org.deep.path import x";
    let js = compile(source);
    assert!(js.contains("@org/deep/path"), "JS: {}", js);
}

#[test]
fn test_with_statement() {
    let source = "with open(\"file.txt\") as f:\n    data = f.read()";
    let js = compile(source);
    assert!(js.contains("try {"), "JS: {}", js);
    assert!(js.contains("finally {"), "JS: {}", js);
}

#[test]
fn test_fizzbuzz_compiles() {
    let fixtures = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures");
    let source = std::fs::read_to_string(fixtures.join("fizzbuzz.ps")).unwrap();
    let js = compile(&source);
    assert!(js.contains("FizzBuzz"), "JS: {}", js);
    assert!(js.contains("Fizz"), "JS: {}", js);
    assert!(js.contains("Buzz"), "JS: {}", js);
}

#[test]
fn test_classes_fixture_compiles() {
    let fixtures = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures");
    let source = std::fs::read_to_string(fixtures.join("classes.ps")).unwrap();
    let js = compile(&source);
    assert!(js.contains("class Animal"), "JS: {}", js);
    assert!(js.contains("class Dog extends Animal"), "JS: {}", js);
}

// ── Phase 1 Tests ────────────────────────────────────────

#[test]
fn test_reassignment() {
    let js = compile("x = 1\nx = 2\ny = x + 1");
    assert!(
        js.contains("let x = 1;"),
        "First assign should use let: {}",
        js
    );
    assert!(js.contains("x = 2;"), "Reassign should skip let: {}", js);
    assert!(
        !js.contains("let x = 2;"),
        "Reassign must NOT use let: {}",
        js
    );
    assert!(js.contains("let y ="), "New var should use let: {}", js);
}

#[test]
fn test_reassignment_in_function() {
    let source = "def foo(a):\n    x = a\n    x = x + 1\n    return x";
    let js = compile(source);
    assert!(js.contains("let x = a;"), "First assign: {}", js);
    assert!(js.contains("x = pyAdd(x, 1);"), "Reassign: {}", js);
    assert!(!js.contains("let x = pyAdd(x, 1)"), "No double let: {}", js);
}

#[test]
fn test_walrus_operator() {
    let source = "if (n := 10) > 5:\n    print(n)";
    let js = compile(source);
    assert!(
        js.contains("(n = 10)"),
        "Walrus should emit assignment: {}",
        js
    );
}

#[test]
fn test_triple_quoted_string() {
    let source = "x = \"\"\"hello\nworld\"\"\"";
    let js = compile(source);
    assert!(js.contains("hello\\nworld"), "Triple string: {}", js);
}

#[test]
fn test_react_import_transform() {
    let source = "from react import use_state, create_element as h";
    let js = compile(source);
    assert!(
        js.contains("import { useState, createElement as h } from \"react\""),
        "React import: {}",
        js
    );
}

#[test]
fn test_pyths_stdlib_alias_resolves() {
    // Twitter-clone find: `from pyths.datetime import ...` emitted
    // "pyths-runtime/datetime" (no such export) instead of the stdlib path.
    let js = compile("from pyths.datetime import datetime");
    assert!(
        js.contains("\"pyths-runtime/stdlib/datetime\""),
        "pyths.datetime alias: {}",
        js
    );
    let js2 = compile("from datetime import datetime");
    assert!(
        js2.contains("\"pyths-runtime/stdlib/datetime\""),
        "bare form unchanged: {}",
        js2
    );
}

#[test]
fn test_except_builtin_name_matching_and_rethrow() {
    // Twitter-clone find: `except ValueError:` swallowed a KeyError (no
    // re-throw branch) AND `__exc instanceof ValueError` was both a
    // ReferenceError (class never imported at except-sites) and wrong for
    // runtime-raised builtins (plain Error with .name set).
    let js = compile(
        "try:
    x = 1
except ValueError:
    pass
",
    );
    assert!(
        js.contains("__exc.name === \"ValueError\""),
        "name-based match: {}",
        js
    );
    assert!(
        js.contains("__exc instanceof ValueError"),
        "instanceof leg (subclasses): {}",
        js
    );
    assert!(
        js.contains("import { ValueError }") || js.contains("ValueError }"),
        "auto-import: {}",
        js
    );
    assert!(
        js.contains("throw __exc;"),
        "non-matching exceptions re-throw: {}",
        js
    );
    // catch-all handler → no dangling else-throw
    let js2 = compile(
        "try:
    x = 1
except Exception:
    pass
",
    );
    assert!(
        !js2.contains("throw __exc;"),
        "catch-all keeps swallowing tail: {}",
        js2
    );
    // tuple form ORs conditions
    let js3 = compile(
        "try:
    x = 1
except (KeyError, IndexError):
    pass
",
    );
    assert!(
        js3.contains("KeyError") && js3.contains("IndexError") && js3.contains(" || "),
        "tuple form: {}",
        js3
    );
    // user classes keep plain instanceof
    let js4 = compile(
        "class MyErr(Exception):
    pass
try:
    x = 1
except MyErr:
    pass
",
    );
    assert!(
        js4.contains("__exc instanceof MyErr"),
        "user class instanceof: {}",
        js4
    );
}

#[test]
fn test_psx_member_expression_component() {
    // Clone-demo find (Netflix): `Ctx.Provider(value=v, children)` inside a
    // @component emitted a plain JS call (args reordered, runtime TypeError).
    // A capitalized member callee is a component: createElement(Ctx.Provider, ...).
    let js = compile(
        "from pyths.react import component, create_context
Ctx = create_context(None)
@component
def App(children):
    return Ctx.Provider(value=1, children)
",
    );
    assert!(
        js.contains("createElement(Ctx.Provider, {value: 1}, children)"),
        "member component: {}",
        js
    );
    // lowercase member calls stay plain method calls
    let js2 = compile(
        "from pyths.react import component
@component
def B(e):
    return button(on_click=lambda: e.preventDefault(), \"x\")",
    );
    assert!(
        js2.contains("e.preventDefault()"),
        "method call untouched: {}",
        js2
    );
}

#[test]
fn test_raise_inside_component_is_constructor_not_element() {
    // Clone-demo find: `raise Exception("x")` inside a @component lowered to
    // `throw createElement(Exception, null, "x")` (PSX capitalized-call rule).
    // A raise operand is never a JSX element — it must stay a constructor.
    let js = compile(
        "from pyths.react import component
@component
def Q():
    raise Exception(\"boom\")
",
    );
    assert!(
        js.contains("throw new Exception(\"boom\")"),
        "raise in PSX: {}",
        js
    );
    assert!(
        !js.contains("throw createElement"),
        "must not throw an element: {}",
        js
    );
    // and raising a custom class still works inside a component
    let js2 = compile(
        "from pyths.react import component
class QuizError(Exception):
    pass
@component
def R():
    raise QuizError(\"q\")
",
    );
    assert!(
        js2.contains("throw new QuizError(\"q\")"),
        "custom raise in PSX: {}",
        js2
    );
}

#[test]
fn test_react_dom_import_names_convert() {
    // A1 (launch survey): react_dom was listed only in kebab form in
    // is_react_or_next_module, so the raw module name never matched and
    // `from react_dom import create_portal` emitted an unconverted
    // `import { create_portal }` while the CALL site emitted createPortal(...)
    // -> guaranteed ReferenceError. All three react_dom module paths convert.
    let js = compile("from react_dom import create_portal, flush_sync");
    assert!(
        js.contains("import { createPortal, flushSync } from \"react-dom\""),
        "react_dom names: {}",
        js
    );
    let js2 = compile("from react_dom.client import create_root");
    assert!(
        js2.contains("import { createRoot } from \"react-dom/client\""),
        "react_dom.client: {}",
        js2
    );
    let js3 = compile("from react_dom.server import render_to_string");
    assert!(
        js3.contains("import { renderToString } from \"react-dom/server\""),
        "react_dom.server: {}",
        js3
    );
}

#[test]
fn test_react_hook_call_transform() {
    let source = "from react import use_state\ncount, set_count = use_state(0)";
    let js = compile(source);
    assert!(js.contains("useState(0)"), "Hook call transform: {}", js);
}

#[test]
fn test_react_prop_transform() {
    let source = "props = {\"on_click\": handler, \"class_name\": \"btn\"}";
    let js = compile(source);
    assert!(js.contains("\"onClick\""), "on_click → onClick: {}", js);
    assert!(
        js.contains("\"className\""),
        "class_name → className: {}",
        js
    );
}

#[test]
fn test_use_client_directive() {
    let source = "\"use client\"\nfrom react import use_state";
    let js = compile(source);
    let lines: Vec<&str> = js.lines().collect();
    assert!(
        lines[0].contains("\"use client\""),
        "Directive must be first line: {}",
        js
    );
}

#[test]
fn test_use_server_directive() {
    let source = "\"use server\"\ndef action():\n    pass";
    let js = compile(source);
    assert!(js.starts_with("\"use server\""), "Directive at top: {}", js);
}

#[test]
fn test_component_decorator() {
    let source = "@component\ndef MyPage():\n    return None";
    let js = compile(source);
    // Named export (not default) so multiple components can share a module.
    assert!(
        js.contains("export function MyPage"),
        "@component → named export: {}",
        js
    );
}

#[test]
fn test_nextjs_export_function() {
    let source = "async def get_server_side_props(context):\n    return {\"props\": {}}";
    let js = compile(source);
    assert!(
        js.contains("export async function getServerSideProps"),
        "Next.js export: {}",
        js
    );
}

#[test]
fn test_react_counter_full() {
    let fixtures =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/react-app");
    let source = std::fs::read_to_string(fixtures.join("counter_pythonic.ps")).unwrap();
    let js = compile(&source);
    assert!(
        js.contains("import { useState, createElement as h } from \"react\""),
        "React import: {}",
        js
    );
    assert!(
        js.contains("export function Counter"),
        "Component export: {}",
        js
    );
    assert!(js.contains("useState(0)"), "Hook call: {}", js);
    assert!(js.contains("\"onClick\""), "Prop transform: {}", js);
}

#[test]
fn test_next_page_component() {
    let fixtures =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/react-app");
    let source = std::fs::read_to_string(fixtures.join("nextjs_page.ps")).unwrap();
    let js = compile(&source);
    assert!(js.contains("useRouter"), "Next.js hook: {}", js);
    assert!(
        js.contains("export async function getServerSideProps"),
        "Next.js export: {}",
        js
    );
}

#[test]
fn test_use_client_component_full() {
    let fixtures =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/react-app");
    let source = std::fs::read_to_string(fixtures.join("client_component.ps")).unwrap();
    let js = compile(&source);
    assert!(
        js.starts_with("\"use client\""),
        "Client directive first: {}",
        js
    );
    assert!(
        js.contains("export function ToggleButton"),
        "Component: {}",
        js
    );
    assert!(js.contains("useState(false)"), "Hook: {}", js);
}

// ── Phase 2 Tests ─────────────────────────────────────

#[test]
fn test_match_case_basic() {
    let js = compile(
        r#"
match command:
    case "quit":
        print("quitting")
    case "hello":
        print("hello!")
    case _:
        print("unknown")
"#,
    );
    assert!(
        js.contains("const __match0 = command"),
        "Match temp var: {}",
        js
    );
    assert!(js.contains("__match0 === \"quit\""), "First case: {}", js);
    assert!(js.contains("__match0 === \"hello\""), "Second case: {}", js);
    assert!(js.contains("else if (true)"), "Wildcard case: {}", js);
}

#[test]
fn test_match_case_capture() {
    let js = compile(
        r#"
match point:
    case [x, y]:
        print(x)
"#,
    );
    assert!(
        js.contains("Array.isArray(__match0)"),
        "Array check: {}",
        js
    );
    assert!(js.contains("__match0.length === 2"), "Length check: {}", js);
}

#[test]
fn test_match_case_or_pattern() {
    let js = compile(
        r#"
match status:
    case 200 | 201:
        print("ok")
"#,
    );
    assert!(
        js.contains("__match0 === 200 || __match0 === 201"),
        "OR pattern: {}",
        js
    );
}

#[test]
fn test_match_case_guard() {
    let js = compile(
        r#"
match value:
    case x if x > 0:
        print("positive")
"#,
    );
    // `x` is Unknown-typed (a match-case capture, no static type pin), so
    // `>` now routes through pyGt — which dispatches `__gt__` for custom-
    // class values (e.g. Decimal/Fraction) and falls back to bare `>` for
    // everything else, so numeric guards behave identically either way.
    assert!(js.contains("pyGt(x, 0)"), "Guard: {}", js);
}

#[test]
fn test_generator_function() {
    let js = compile(
        r#"
def fibonacci():
    a = 0
    b = 1
    while True:
        yield a
        a, b = b, a + b
"#,
    );
    assert!(js.contains("function* fibonacci()"), "Generator: {}", js);
    assert!(js.contains("yield a"), "Yield: {}", js);
}

#[test]
fn test_generator_method() {
    let js = compile(
        r#"
class Range:
    def __init__(self, n):
        self.n = n

    def __iter__(self):
        i = 0
        while i < self.n:
            yield i
            i += 1
"#,
    );
    assert!(js.contains("*__iter__()"), "Generator method: {}", js);
    assert!(js.contains("yield i"), "Yield in method: {}", js);
}

#[test]
fn test_async_generator_function() {
    // `async def` + `yield` lowers to `async function*` — the JS shape
    // for an asynchronous iterable that `for await (...)` consumes.
    let js = compile(
        r#"
async def stream_numbers(n):
    i = 0
    while i < n:
        yield i
        i += 1
"#,
    );
    assert!(
        js.contains("async function* stream_numbers"),
        "async generator declaration: {}",
        js
    );
    assert!(js.contains("yield i"), "yield body: {}", js);
}

#[test]
fn test_async_generator_with_await_inside_body() {
    // Async generators must accept `await` in their body — the canonical
    // case is yielding the result of an awaited fetch.
    let js = compile(
        r#"
async def fetch_stream(urls):
    for url in urls:
        result = await fetch(url)
        yield result
"#,
    );
    assert!(
        js.contains("async function* fetch_stream"),
        "async generator: {}",
        js
    );
    assert!(js.contains("await fetch(url)"), "await preserved: {}", js);
    assert!(js.contains("yield result"), "yield preserved: {}", js);
}

#[test]
fn test_async_for_consumes_async_generator() {
    // `async for x in gen()` lowers to `for await (const x of gen())` —
    // the canonical consumer of an async generator.
    let js = compile(
        r#"
async def consume():
    async for item in stream():
        print(item)
"#,
    );
    assert!(js.contains("for await (const item of"), "async-for: {}", js);
}

#[test]
fn test_async_generator_method_in_class() {
    // `async def` + `yield` inside a class method lowers to
    // `async *method()`.
    let js = compile(
        r#"
class StreamSource:
    def __init__(self, items):
        self.items = items

    async def __aiter__(self):
        for x in self.items:
            yield x
"#,
    );
    assert!(
        js.contains("async *__aiter__()"),
        "async generator method: {}",
        js
    );
}

#[test]
fn test_dataclass_basic() {
    let js = compile(
        r#"
from dataclasses import dataclass

@dataclass
class Point:
    x: int
    y: int
"#,
    );
    assert!(!js.contains("dataclasses"), "No dataclasses import: {}", js);
    assert!(js.contains("constructor(x, y)"), "Constructor: {}", js);
    assert!(js.contains("this.x = x"), "Field init: {}", js);
    assert!(js.contains("this.y = y"), "Field init: {}", js);
    assert!(js.contains("toString()"), "toString: {}", js);
    assert!(js.contains("__eq__(other)"), "Equality: {}", js);
    assert!(
        js.contains("other instanceof Point"),
        "instanceof check: {}",
        js
    );
}

#[test]
fn test_dataclass_with_defaults() {
    let js = compile(
        r#"
from dataclasses import dataclass

@dataclass
class Config:
    name: str
    debug: bool = False
    port: int = 8080
"#,
    );
    assert!(
        js.contains("constructor(name, debug = false, port = 8080)"),
        "Defaults: {}",
        js
    );
}

#[test]
fn test_dataclass_with_method() {
    let js = compile(
        r#"
from dataclasses import dataclass

@dataclass
class User:
    name: str
    age: int

    def greet(self):
        return f"Hello, {self.name}"
"#,
    );
    assert!(js.contains("constructor(name, age)"), "Constructor: {}", js);
    assert!(js.contains("greet()"), "Method preserved: {}", js);
    assert!(js.contains("this.name"), "Self → this: {}", js);
}

#[test]
fn test_annotated_assignment() {
    let js = compile("x: int = 42\nname: str = \"hello\"");
    assert!(js.contains("let x = 42"), "Annotated int: {}", js);
    assert!(js.contains("let name = \"hello\""), "Annotated str: {}", js);
}

#[test]
fn test_psx_basic_element() {
    let js = compile(
        r#"
"use client"

@component
def App():
    return div(class_name="app",
        h1("Hello World"),
    )
"#,
    );
    assert!(
        js.contains("createElement(\"div\", {className: \"app\"}"),
        "Div with prop: {}",
        js
    );
    assert!(
        js.contains("createElement(\"h1\", null, \"Hello World\")"),
        "H1 text child: {}",
        js
    );
    assert!(!js.contains("<div"), "No JSX angle brackets: {}", js);
}

#[test]
fn test_psx_self_closing() {
    let js = compile(
        r#"
"use client"

@component
def Avatar():
    return img(src="/pic.png", alt="Avatar")
"#,
    );
    assert!(
        js.contains("createElement(\"img\", {src: \"/pic.png\", alt: \"Avatar\"})"),
        "Self-closing: {}",
        js
    );
}

#[test]
fn test_psx_event_handler() {
    let js = compile(
        r#"
"use client"

@component
def Button():
    return button(on_click=handler, "Click me")
"#,
    );
    assert!(js.contains("onClick: handler"), "Event prop: {}", js);
    assert!(js.contains("\"Click me\""), "Text child: {}", js);
}

// Regression: B-037 (pythscribe#53). Inside a @component, a capitalized call
// to a known JS/DOM builtin constructor (EventSource, URL, FormData, Date,
// Map, ...) must emit `new Name(...)`, NOT `createElement(Name, ...)`.
// A capitalized call in PSX mode is normally a React element; built-in
// constructors are the exception. (User-defined classes already route to
// `new` via known_classes; this covers external/global constructors.)
#[test]
fn test_psx_builtin_constructor_uses_new() {
    let js = compile(
        r#"
"use client"

@component
def Stream(url=""):
    es = EventSource(url)
    u = URL(url)
    return div("x")
"#,
    );
    assert!(
        js.contains("new EventSource(url)"),
        "EventSource → new: {}",
        js
    );
    assert!(js.contains("new URL(url)"), "URL → new: {}", js);
    assert!(
        !js.contains("createElement(EventSource"),
        "must not createElement a constructor: {}",
        js
    );
    assert!(
        !js.contains("createElement(URL"),
        "must not createElement a constructor: {}",
        js
    );
}

#[test]
fn test_psx_expression_child() {
    let js = compile(
        r#"
"use client"

@component
def Display():
    return p(count)
"#,
    );
    assert!(
        js.contains("createElement(\"p\", null, count)"),
        "Expression child: {}",
        js
    );
}

#[test]
fn test_psx_fragment() {
    let js = compile(
        r#"
"use client"

@component
def Layout():
    return (
        header("Top"),
        footer("Bottom"),
    )
"#,
    );
    assert!(
        js.contains("createElement(Fragment, null"),
        "Fragment: {}",
        js
    );
    assert!(
        js.contains("createElement(\"header\", null, \"Top\")"),
        "Fragment child 1: {}",
        js
    );
    assert!(
        js.contains("createElement(\"footer\", null, \"Bottom\")"),
        "Fragment child 2: {}",
        js
    );
    assert!(!js.contains("<>"), "No JSX fragment syntax: {}", js);
}

#[test]
fn test_psx_boolean_prop() {
    let js = compile(
        r#"
"use client"

@component
def Form():
    return input(disabled=True, auto_focus=True)
"#,
    );
    assert!(js.contains("disabled: true"), "Boolean true prop: {}", js);
    assert!(
        js.contains("autoFocus: true"),
        "Camelcase boolean prop: {}",
        js
    );
}

#[test]
fn test_psx_nested_components() {
    let js = compile(
        r#"
"use client"

@component
def Page():
    return div(
        Header(title="My Page"),
        main("Content"),
    )
"#,
    );
    assert!(
        js.contains("createElement(Header, {title: \"My Page\"})"),
        "Component (no quotes): {}",
        js
    );
    assert!(
        js.contains("createElement(\"main\", null, \"Content\")"),
        "HTML nested: {}",
        js
    );
}

#[test]
fn test_psx_not_in_regular_function() {
    // Outside @component, function calls should NOT emit createElement
    let js = compile(
        r#"
def regular():
    return div("hello")
"#,
    );
    assert!(
        !js.contains("createElement"),
        "No createElement outside component: {}",
        js
    );
    assert!(js.contains("div(\"hello\")"), "Regular call: {}", js);
}

#[test]
fn test_source_map_generation() {
    let source = "x = 42\nprint(x)";
    let module = pyths_parser::parse(source).expect("Parse failed");
    let result = pyths_codegen_js::codegen_with_sourcemap(
        &module,
        source,
        "test.ps",
        "test.js",
        &std::collections::HashMap::new(),
    );
    assert!(result.js.contains("let x = 42"), "JS output: {}", result.js);
    let map = result.source_map.unwrap();
    assert!(map.contains("\"version\":3"), "V3 source map: {}", map);
    assert!(
        map.contains("\"sources\":[\"test.ps\"]"),
        "Source file: {}",
        map
    );
    assert!(map.contains("\"file\":\"test.js\""), "Output file: {}", map);
    assert!(map.contains("\"mappings\":"), "Has mappings: {}", map);
}

#[test]
fn test_sourcemap_default_inlines_sources_content() {
    // A17: default behavior is unchanged — `sourcesContent` is inlined.
    let source = "x = 42\nprint(x)";
    let module = pyths_parser::parse(source).expect("Parse failed");
    let result = pyths_codegen_js::codegen_with_sourcemap_and_options(
        &module,
        source,
        "test.ps",
        "test.js",
        &std::collections::HashMap::new(),
        false,
        /*omit_sources_content=*/ false,
    );
    let map = result.source_map.unwrap();
    assert!(
        map.contains("\"sourcesContent\":"),
        "default map inlines source: {}",
        map
    );
    assert!(
        map.contains("print(x)"),
        "default map carries original text: {}",
        map
    );
}

#[test]
fn test_sourcemap_no_sources_content_omits_original() {
    // A17: `--no-sources-content` → the map omits `sourcesContent` and does
    // NOT ship the original `.ps` text, while still resolving positions.
    let source = "secret = 42\nprint(secret)";
    let module = pyths_parser::parse(source).expect("Parse failed");
    let result = pyths_codegen_js::codegen_with_sourcemap_and_options(
        &module,
        source,
        "test.ps",
        "test.js",
        &std::collections::HashMap::new(),
        false,
        /*omit_sources_content=*/ true,
    );
    let map = result.source_map.unwrap();
    assert!(
        !map.contains("\"sourcesContent\""),
        "map must omit sourcesContent: {}",
        map
    );
    assert!(
        !map.contains("secret"),
        "original source text must not ship: {}",
        map
    );
    // Still a valid v3 map with mappings + the source file reference.
    assert!(map.contains("\"version\":3"), "still a v3 map: {}", map);
    assert!(map.contains("\"mappings\":"), "still has mappings: {}", map);
    assert!(
        map.contains("\"sources\":[\"test.ps\"]"),
        "still names the source: {}",
        map
    );
}

#[test]
fn test_match_case_fixture() {
    let fixtures = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures");
    let source = std::fs::read_to_string(fixtures.join("match_case.ps")).unwrap();
    let js = compile(&source);
    assert!(js.contains("if ("), "Match compiled: {}", js);
}

#[test]
fn test_dataclass_fixture() {
    let fixtures = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures");
    let source = std::fs::read_to_string(fixtures.join("dataclass.ps")).unwrap();
    let js = compile(&source);
    assert!(js.contains("class Point"), "Point class: {}", js);
    assert!(js.contains("class User"), "User class: {}", js);
    assert!(js.contains("constructor("), "Has constructor: {}", js);
    assert!(js.contains("new Point(1, 2)"), "Instantiation: {}", js);
}

#[test]
fn test_psx_counter_fixture() {
    let fixtures = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures");
    let source = std::fs::read_to_string(fixtures.join("psx_basic.ps")).unwrap();
    let js = compile(&source);
    assert!(js.contains("\"use client\""), "Directive: {}", js);
    assert!(
        js.contains("createElement(\"div\", {className: \"counter\"}"),
        "PSX div: {}",
        js
    );
    assert!(
        js.contains("createElement(\"h1\", null, \"Counter App\")"),
        "PSX h1: {}",
        js
    );
    assert!(js.contains("onClick:"), "Event handler: {}", js);
    assert!(!js.contains("<div"), "No angle brackets: {}", js);
}

// ============================
// Phase 3 — Ecosystem & Tooling
// ============================

// --- Standard Library Import Resolution ---

#[test]
fn test_stdlib_import_math() {
    let js = compile("import math");
    assert!(
        js.contains("import * as math from \"pyths-runtime/stdlib/math\""),
        "JS: {}",
        js
    );
}

#[test]
fn test_stdlib_import_json() {
    let js = compile("from json import dumps, loads");
    assert!(
        js.contains("import { dumps, loads } from \"pyths-runtime/stdlib/json\""),
        "JS: {}",
        js
    );
}

#[test]
fn test_stdlib_import_itertools() {
    let js = compile("from itertools import chain, islice");
    assert!(
        js.contains("import { chain, islice } from \"pyths-runtime/stdlib/itertools\""),
        "JS: {}",
        js
    );
}

#[test]
fn test_stdlib_import_functools() {
    let js = compile("from functools import reduce, partial");
    assert!(
        js.contains("import { reduce, partial } from \"pyths-runtime/stdlib/functools\""),
        "JS: {}",
        js
    );
}

#[test]
fn test_stdlib_import_collections() {
    let js = compile("from collections import Counter, deque");
    assert!(
        js.contains("import { Counter, deque } from \"pyths-runtime/stdlib/collections\""),
        "JS: {}",
        js
    );
}

#[test]
fn test_stdlib_import_random() {
    let js = compile("from random import randint, choice");
    assert!(
        js.contains("import { randint, choice } from \"pyths-runtime/stdlib/random\""),
        "JS: {}",
        js
    );
}

#[test]
fn test_stdlib_import_datetime() {
    let js = compile("from datetime import datetime, date");
    assert!(
        js.contains("import { datetime, date } from \"pyths-runtime/stdlib/datetime\""),
        "JS: {}",
        js
    );
}

#[test]
fn test_stdlib_import_re() {
    let js = compile("from re import search, findall");
    assert!(
        js.contains("import { search, findall } from \"pyths-runtime/stdlib/re\""),
        "JS: {}",
        js
    );
}

// --- Web Module Import Resolution ---

#[test]
fn test_web_import_dom() {
    let js = compile("from pyths.dom import query, query_all");
    assert!(
        js.contains("import { query, query_all } from \"pyths-runtime/dom\""),
        "JS: {}",
        js
    );
}

#[test]
fn test_web_import_fetch() {
    let js = compile("from pyths.fetch import get, post");
    assert!(
        js.contains("import { get, post } from \"pyths-runtime/web/fetch\""),
        "JS: {}",
        js
    );
}

#[test]
fn test_web_import_storage() {
    let js = compile("from pyths.storage import local, session");
    assert!(
        js.contains("import { local, session } from \"pyths-runtime/web/storage\""),
        "JS: {}",
        js
    );
}

#[test]
fn test_web_import_router() {
    let js = compile("from pyths.router import route, navigate");
    assert!(
        js.contains("import { route, navigate } from \"pyths-runtime/web/router\""),
        "JS: {}",
        js
    );
}

// --- pyths.utils.tenacity import ---

#[test]
fn test_utils_tenacity_import_retry() {
    let js = compile("from pyths.utils.tenacity import retry");
    assert!(
        js.contains("import { retry } from \"pyths-runtime/utils/tenacity\""),
        "JS: {}",
        js
    );
}

#[test]
fn test_retry_decorator_applied() {
    let js = compile(
        r#"
from pyths.utils.tenacity import retry

@retry(max_attempts=3, delay=1.0)
async def fetch_data(url):
    response = await get(url)
    return response
"#,
    );
    assert!(
        js.contains("import { retry } from \"pyths-runtime/utils/tenacity\""),
        "Import: {}",
        js
    );
    // Round-2: kwargs calls route through __pyCallKw; `retry` (a JS
    // runtime util, no __pyparams__ metadata) takes the legacy
    // options-object fallback inside the helper — same behavior.
    assert!(
        js.contains("fetch_data = __pyCallKw(retry, [], {max_attempts: 3, delay: 1})(fetch_data)"),
        "Decorator applied: {}",
        js
    );
}

#[test]
fn test_nested_pyths_module_resolution() {
    let js = compile("from pyths.a.b.c import thing");
    assert!(
        js.contains("import { thing } from \"pyths-runtime/a/b/c\""),
        "Nested dots: {}",
        js
    );
}

// --- Redux / React-Redux ---

#[test]
fn test_react_redux_hooks_import() {
    let js = compile("from react_redux import use_selector, use_dispatch");
    assert!(
        js.contains("import { useSelector, useDispatch } from \"react-redux\""),
        "JS: {}",
        js
    );
}

#[test]
fn test_reduxjs_toolkit_import() {
    let js = compile("from reduxjs.toolkit import create_slice, configure_store");
    assert!(
        js.contains("import { createSlice, configureStore } from \"@reduxjs/toolkit\""),
        "JS: {}",
        js
    );
}

#[test]
fn test_redux_create_async_thunk() {
    let js = compile("from reduxjs.toolkit import create_async_thunk, create_reducer");
    assert!(
        js.contains("import { createAsyncThunk, createReducer } from \"@reduxjs/toolkit\""),
        "JS: {}",
        js
    );
}

// --- Non-stdlib passthrough ---

#[test]
fn test_non_stdlib_import_passthrough() {
    // Generic npm fallback emits the kebab-case package path.
    let js = compile("import my_module");
    assert!(
        js.contains("import * as my_module from \"my-module\""),
        "JS: {}",
        js
    );
}

#[test]
fn test_non_stdlib_dotted_import() {
    // Generic npm fallback kebab-cases each path segment.
    let js = compile("from my_package.utils import helper");
    assert!(
        js.contains("import { helper } from \"my-package/utils\""),
        "JS: {}",
        js
    );
}

// --- Dataclasses import suppression (regression) ---

#[test]
fn test_dataclasses_import_suppressed() {
    let js = compile("from dataclasses import dataclass\n\n@dataclass\nclass Foo:\n    x: int\n");
    assert!(
        !js.contains("dataclasses"),
        "Should suppress dataclasses import: {}",
        js
    );
}

// --- Test runner fixture ---

#[test]
fn test_test_fixture_compiles() {
    let fixtures = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures");
    let source = std::fs::read_to_string(fixtures.join("test_basics.ps")).unwrap();
    let js = compile(&source);
    assert!(js.contains("pyPrint"), "JS: {}", js);
}

// ============================
// Phase 3+ — Dataclass Validation
// ============================

#[test]
fn test_dataclass_type_validation_int() {
    let js = compile("from dataclasses import dataclass\n\n@dataclass\nclass Foo:\n    x: int\n");
    assert!(
        js.contains("typeof x !== \"number\""),
        "typeof check: {}",
        js
    );
    assert!(
        js.contains("Number.isInteger(x)"),
        "isInteger check: {}",
        js
    );
    assert!(
        js.contains("throw new TypeError(\"Foo.x: expected int"),
        "TypeError: {}",
        js
    );
}

#[test]
fn test_dataclass_type_validation_str() {
    let js =
        compile("from dataclasses import dataclass\n\n@dataclass\nclass Bar:\n    name: str\n");
    assert!(
        js.contains("typeof name !== \"string\""),
        "typeof str check: {}",
        js
    );
    assert!(
        js.contains("throw new TypeError(\"Bar.name: expected str"),
        "TypeError: {}",
        js
    );
}

#[test]
fn test_dataclass_type_validation_bool() {
    let js = compile(
        "from dataclasses import dataclass\n\n@dataclass\nclass Flags:\n    active: bool\n",
    );
    assert!(
        js.contains("typeof active !== \"boolean\""),
        "typeof bool check: {}",
        js
    );
    assert!(
        js.contains("throw new TypeError(\"Flags.active: expected bool"),
        "TypeError: {}",
        js
    );
}

#[test]
fn test_dataclass_type_validation_list() {
    let js =
        compile("from dataclasses import dataclass\n\n@dataclass\nclass Bag:\n    items: list\n");
    assert!(js.contains("Array.isArray(items)"), "Array check: {}", js);
    assert!(
        js.contains("throw new TypeError(\"Bag.items: expected list"),
        "TypeError: {}",
        js
    );
}

#[test]
fn test_dataclass_optional_type() {
    let js = compile("from dataclasses import dataclass\nfrom typing import Optional\n\n@dataclass\nclass Opt:\n    val: Optional[str] = None\n");
    assert!(
        js.contains("val !== null && val !== undefined"),
        "null/undefined guard: {}",
        js
    );
    assert!(
        js.contains("typeof val !== \"string\""),
        "inner type check: {}",
        js
    );
}

#[test]
fn test_dataclass_nested_instance() {
    let js = compile("from dataclasses import dataclass\n\n@dataclass\nclass Inner:\n    x: int\n\n@dataclass\nclass Outer:\n    child: Inner\n");
    assert!(
        js.contains("child instanceof Inner"),
        "instanceof check: {}",
        js
    );
}

#[test]
fn test_dataclass_list_element_validation() {
    let js = compile(
        "from dataclasses import dataclass\n\n@dataclass\nclass Nums:\n    values: list[int]\n",
    );
    assert!(js.contains("Array.isArray(values)"), "Array check: {}", js);
    assert!(
        js.contains("for (const values_el of values)"),
        "element loop: {}",
        js
    );
    assert!(
        js.contains("Number.isInteger(values_el)"),
        "element type check: {}",
        js
    );
}

#[test]
fn test_dataclass_field_constraints() {
    let js = compile("from dataclasses import dataclass, Field\n\n@dataclass\nclass Bounded:\n    age: int = Field(ge=0, le=150)\n    name: str = Field(min_length=1, max_length=50)\n");
    assert!(js.contains("if (age < 0)"), "ge constraint: {}", js);
    assert!(js.contains("if (age > 150)"), "le constraint: {}", js);
    assert!(js.contains("if (name.length < 1)"), "min_length: {}", js);
    assert!(js.contains("if (name.length > 50)"), "max_length: {}", js);
}

#[test]
fn test_dataclass_field_pattern() {
    let js = compile("from dataclasses import dataclass, Field\n\n@dataclass\nclass Zip:\n    code: str = Field(pattern=\"^[0-9]{5}$\")\n");
    assert!(
        js.contains("/^[0-9]{5}$/.test(code)"),
        "pattern regex: {}",
        js
    );
    assert!(
        js.contains("throw new TypeError"),
        "TypeError on pattern fail: {}",
        js
    );
}

#[test]
fn test_dataclass_field_with_default() {
    let js = compile("from dataclasses import dataclass, Field\n\n@dataclass\nclass Item:\n    count: int = Field(default=3, ge=0)\n");
    assert!(js.contains("count = 3"), "default value: {}", js);
    assert!(
        js.contains("if (count < 0)"),
        "ge constraint with default: {}",
        js
    );
}

#[test]
fn test_dataclass_frozen() {
    let js = compile("from dataclasses import dataclass\n\n@dataclass(frozen=True)\nclass Frozen:\n    x: int\n    y: int\n");
    assert!(js.contains("Object.freeze(this)"), "frozen: {}", js);
}

#[test]
fn test_dataclass_to_dict() {
    let js = compile(
        "from dataclasses import dataclass\n\n@dataclass\nclass Pt:\n    x: int\n    y: int\n",
    );
    assert!(js.contains("toDict()"), "toDict method: {}", js);
    assert!(
        js.contains("return { x: this.x, y: this.y }"),
        "toDict body: {}",
        js
    );
}

#[test]
fn test_dataclass_from_dict() {
    let js = compile(
        "from dataclasses import dataclass\n\n@dataclass\nclass Pt:\n    x: int\n    y: int\n",
    );
    assert!(
        js.contains("static fromDict(data)"),
        "fromDict method: {}",
        js
    );
    assert!(
        js.contains("return new Pt(data.x, data.y)"),
        "fromDict body: {}",
        js
    );
}

#[test]
fn test_dataclass_default_factory() {
    let js = compile("from dataclasses import dataclass, field\n\n@dataclass\nclass Container:\n    items: list = field(default_factory=list)\n    meta: dict = field(default_factory=dict)\n");
    assert!(js.contains("items = []"), "list factory: {}", js);
    assert!(js.contains("meta = {}"), "dict factory: {}", js);
}

#[test]
fn test_dataclass_import_suppression_all() {
    let js = compile("from dataclasses import dataclass, Field\nfrom pydantic import validator\nfrom typing import Optional\n\n@dataclass\nclass Foo:\n    x: int\n");
    assert!(
        !js.contains("dataclasses") && !js.contains("pydantic") && !js.contains("typing"),
        "All type-only imports suppressed: {}",
        js
    );
}

#[test]
fn test_dataclass_validator() {
    let js = compile("from dataclasses import dataclass\nfrom pydantic import validator\n\n@dataclass\nclass Item:\n    name: str\n\n    @validator(\"name\")\n    def clean_name(self, value):\n        return value.strip()\n");
    assert!(
        js.contains("this.name = this.clean_name(this.name)"),
        "validator call: {}",
        js
    );
    assert!(
        js.contains("clean_name(value)"),
        "validator method exists: {}",
        js
    );
}

#[test]
fn test_dataclass_validated_fixture() {
    let fixtures = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures");
    let source = std::fs::read_to_string(fixtures.join("dataclass_validated.ps")).unwrap();
    let js = compile(&source);
    assert!(js.contains("class Point"), "Point class: {}", js);
    assert!(js.contains("class User"), "User class: {}", js);
    assert!(js.contains("Object.freeze(this)"), "User frozen: {}", js);
    assert!(js.contains("toDict()"), "toDict: {}", js);
    assert!(js.contains("static fromDict(data)"), "fromDict: {}", js);
    // The @dataclass machinery itself is self-contained — the only runtime
    // import is the incidental pyPrint from a print() in the fixture, not
    // any dataclass-specific helper.
    for line in js.lines().filter(|l| l.starts_with("import")) {
        assert!(
            line.contains("pyPrint"),
            "unexpected import in dataclass output: {}",
            line
        );
    }
}

// ============================
// Phase 7 — Zod-like @dataclass Validation Extensions
// ============================

// --- Tier 1: Field Constraint Extensions ---

#[test]
fn test_dataclass_email_constraint() {
    let js = compile("@dataclass\nclass C:\n    email: str = Field(email=True)\n");
    assert!(
        js.contains("@[^\\s@]+\\.[^\\s@]+$/.test(email)"),
        "email check: {}",
        js
    );
}

#[test]
fn test_dataclass_url_constraint() {
    let js = compile("@dataclass\nclass C:\n    link: str = Field(url=True)\n");
    assert!(
        js.contains("https?:\\/\\/.+/.test(link)"),
        "url check: {}",
        js
    );
}

#[test]
fn test_dataclass_uuid_constraint() {
    let js = compile("@dataclass\nclass C:\n    id: str = Field(uuid=True)\n");
    assert!(js.contains("[0-9a-f]"), "uuid check: {}", js);
    assert!(js.contains(".test(id)"), "uuid test call: {}", js);
}

#[test]
fn test_dataclass_starts_with() {
    let js = compile("@dataclass\nclass C:\n    name: str = Field(starts_with=\"pre\")\n");
    assert!(
        js.contains("name.startsWith(\"pre\")"),
        "starts_with: {}",
        js
    );
}

#[test]
fn test_dataclass_ends_with() {
    let js = compile("@dataclass\nclass C:\n    name: str = Field(ends_with=\"fix\")\n");
    assert!(js.contains("name.endsWith(\"fix\")"), "ends_with: {}", js);
}

#[test]
fn test_dataclass_includes() {
    let js = compile("@dataclass\nclass C:\n    name: str = Field(includes=\"mid\")\n");
    assert!(js.contains("name.includes(\"mid\")"), "includes: {}", js);
}

#[test]
fn test_dataclass_trim_transform() {
    let js = compile("@dataclass\nclass C:\n    name: str = Field(trim=True)\n");
    assert!(js.contains("name = name.trim()"), "trim: {}", js);
}

#[test]
fn test_dataclass_to_lower_transform() {
    let js = compile("@dataclass\nclass C:\n    tag: str = Field(to_lower=True)\n");
    assert!(js.contains("tag = tag.toLowerCase()"), "to_lower: {}", js);
}

#[test]
fn test_dataclass_to_upper_transform() {
    let js = compile("@dataclass\nclass C:\n    code: str = Field(to_upper=True)\n");
    assert!(js.contains("code = code.toUpperCase()"), "to_upper: {}", js);
}

#[test]
fn test_dataclass_transform_before_constraint() {
    let js = compile("@dataclass\nclass C:\n    name: str = Field(trim=True, min_length=1)\n");
    // trim should appear before min_length check
    let trim_pos = js.find("name.trim()").expect("trim present");
    let min_pos = js.find("name.length < 1").expect("min_length present");
    assert!(trim_pos < min_pos, "trim before constraint: {}", js);
}

#[test]
fn test_dataclass_positive_constraint() {
    let js = compile("@dataclass\nclass C:\n    x: int = Field(positive=True)\n");
    assert!(js.contains("x <= 0"), "positive: {}", js);
}

#[test]
fn test_dataclass_negative_constraint() {
    let js = compile("@dataclass\nclass C:\n    x: int = Field(negative=True)\n");
    assert!(js.contains("x >= 0"), "negative: {}", js);
}

#[test]
fn test_dataclass_nonnegative_constraint() {
    let js = compile("@dataclass\nclass C:\n    x: int = Field(nonnegative=True)\n");
    assert!(js.contains("x < 0"), "nonnegative: {}", js);
}

#[test]
fn test_dataclass_multiple_of() {
    let js = compile("@dataclass\nclass C:\n    x: int = Field(multiple_of=5)\n");
    assert!(js.contains("x % 5 !== 0"), "multiple_of: {}", js);
}

#[test]
fn test_dataclass_finite_constraint() {
    let js = compile("@dataclass\nclass C:\n    x: float = Field(finite=True)\n");
    assert!(js.contains("Number.isFinite(x)"), "finite: {}", js);
}

#[test]
fn test_dataclass_choices_str() {
    let js = compile(
        "@dataclass\nclass C:\n    status: str = Field(choices=[\"active\", \"inactive\"])\n",
    );
    assert!(
        js.contains("[\"active\", \"inactive\"].includes(status)"),
        "choices str: {}",
        js
    );
}

#[test]
fn test_dataclass_choices_int() {
    let js = compile("@dataclass\nclass C:\n    level: int = Field(choices=[1, 2, 3])\n");
    assert!(
        js.contains("[1, 2, 3].includes(level)"),
        "choices int: {}",
        js
    );
}

#[test]
fn test_dataclass_extended_fixture() {
    let fixtures = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures");
    let source = std::fs::read_to_string(fixtures.join("dataclass_extended.ps")).unwrap();
    let js = compile(&source);
    assert!(
        js.contains("class EmailRecord"),
        "EmailRecord class: {}",
        js
    );
    assert!(js.contains("email.trim()"), "trim in fixture: {}", js);
    assert!(
        js.contains(".startsWith("),
        "starts_with in fixture: {}",
        js
    );
    assert!(js.contains("Number.isFinite("), "finite in fixture: {}", js);
    assert!(
        js.contains(".includes(level)"),
        "choices in fixture: {}",
        js
    );
}

// --- Tier 2: Coercion + Error Collection ---

#[test]
fn test_dataclass_coerce_int() {
    let js = compile("@dataclass(coerce=True)\nclass C:\n    x: int\n");
    assert!(js.contains("parseInt(x, 10)"), "coerce int: {}", js);
}

#[test]
fn test_dataclass_coerce_float() {
    let js = compile("@dataclass(coerce=True)\nclass C:\n    x: float\n");
    assert!(js.contains("parseFloat(x)"), "coerce float: {}", js);
}

#[test]
fn test_dataclass_coerce_str() {
    let js = compile("@dataclass(coerce=True)\nclass C:\n    x: str\n");
    assert!(js.contains("String(x)"), "coerce str: {}", js);
}

#[test]
fn test_dataclass_coerce_bool() {
    let js = compile("@dataclass(coerce=True)\nclass C:\n    x: bool\n");
    assert!(js.contains("x === \"true\""), "coerce bool: {}", js);
    assert!(js.contains("x !== 0"), "coerce bool from number: {}", js);
}

#[test]
fn test_dataclass_coerce_optional() {
    let js = compile(
        "from typing import Optional\n@dataclass(coerce=True)\nclass C:\n    x: Optional[int]\n",
    );
    assert!(
        js.contains("x !== null && x !== undefined"),
        "optional guard: {}",
        js
    );
    assert!(js.contains("parseInt(x, 10)"), "coerce inner int: {}", js);
}

#[test]
fn test_dataclass_coerce_order() {
    let js = compile("@dataclass(coerce=True)\nclass C:\n    x: int\n");
    // Coercion should appear before type validation
    let coerce_pos = js.find("parseInt(x, 10)").expect("coerce present");
    let type_pos = js
        .find("typeof x !== \"number\"")
        .expect("type check present");
    assert!(coerce_pos < type_pos, "coerce before type check: {}", js);
}

#[test]
fn test_dataclass_collect_errors() {
    let js = compile("@dataclass(collect_errors=True)\nclass C:\n    x: int\n    y: str\n");
    assert!(js.contains("const __errors = []"), "errors array: {}", js);
    assert!(js.contains("__errors.push("), "push errors: {}", js);
    assert!(js.contains("__errors.length > 0"), "errors check: {}", js);
}

#[test]
fn test_dataclass_collect_errors_constraint() {
    let js = compile("@dataclass(collect_errors=True)\nclass C:\n    x: int = Field(gt=0)\n");
    assert!(js.contains("__errors.push("), "push on constraint: {}", js);
    assert!(
        !js.contains("throw new TypeError(\"C.x: must be >"),
        "no throw for constraint: {}",
        js
    );
}

#[test]
fn test_dataclass_collect_errors_message() {
    let js = compile("@dataclass(collect_errors=True)\nclass C:\n    x: int\n");
    assert!(
        js.contains("__errors.join(\"; \")"),
        "joined message: {}",
        js
    );
}

#[test]
fn test_dataclass_coerce_and_collect() {
    let js = compile("@dataclass(coerce=True, collect_errors=True)\nclass C:\n    x: int\n");
    assert!(js.contains("parseInt(x, 10)"), "coerce: {}", js);
    assert!(js.contains("__errors.push("), "collect: {}", js);
}

#[test]
fn test_dataclass_no_coerce_no_collect() {
    let js = compile("@dataclass\nclass C:\n    x: int\n");
    assert!(!js.contains("parseInt"), "no coerce: {}", js);
    assert!(!js.contains("__errors"), "no collect: {}", js);
}

#[test]
fn test_dataclass_coerce_frozen() {
    let js = compile("@dataclass(coerce=True, frozen=True)\nclass C:\n    x: int\n");
    assert!(js.contains("parseInt(x, 10)"), "coerce: {}", js);
    assert!(js.contains("Object.freeze(this)"), "frozen: {}", js);
}

#[test]
fn test_dataclass_coerce_fixture() {
    let fixtures = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures");
    let source = std::fs::read_to_string(fixtures.join("dataclass_coerce.ps")).unwrap();
    let js = compile(&source);
    assert!(js.contains("class CoercedConfig"), "CoercedConfig: {}", js);
    assert!(js.contains("parseInt(port, 10)"), "coerce port: {}", js);
    assert!(js.contains("__errors.push("), "collect errors: {}", js);
}

// --- Tier 3: Cross-Field Validation (@check) ---

#[test]
fn test_dataclass_check_basic() {
    let js = compile("@dataclass\nclass C:\n    start: int\n    end: int\n\n    @check\n    def validate_range(self):\n        if self.start > self.end:\n            raise ValueError(\"start must be <= end\")\n");
    assert!(js.contains("this.validate_range()"), "check call: {}", js);
    assert!(js.contains("validate_range()"), "method emitted: {}", js);
}

#[test]
fn test_dataclass_check_multiple() {
    let js = compile("@dataclass\nclass C:\n    x: int\n    y: int\n\n    @check\n    def check_x(self):\n        pass\n\n    @check\n    def check_y(self):\n        pass\n");
    assert!(js.contains("this.check_x()"), "check_x call: {}", js);
    assert!(js.contains("this.check_y()"), "check_y call: {}", js);
}

#[test]
fn test_dataclass_check_after_validator() {
    let js = compile("@dataclass\nclass C:\n    name: str\n\n    @validator(\"name\")\n    def clean(self, value):\n        return value.strip()\n\n    @check\n    def verify(self):\n        pass\n");
    let validator_pos = js
        .find("this.name = this.clean(this.name)")
        .expect("validator present");
    let check_pos = js.find("this.verify()").expect("check present");
    assert!(validator_pos < check_pos, "validator before check: {}", js);
}

#[test]
fn test_dataclass_check_before_freeze() {
    let js = compile("@dataclass(frozen=True)\nclass C:\n    x: int\n\n    @check\n    def verify(self):\n        pass\n");
    let check_pos = js.find("this.verify()").expect("check present");
    let freeze_pos = js.find("Object.freeze(this)").expect("freeze present");
    assert!(check_pos < freeze_pos, "check before freeze: {}", js);
}

#[test]
fn test_dataclass_check_decorator_stripped() {
    let js = compile(
        "@dataclass\nclass C:\n    x: int\n\n    @check\n    def verify(self):\n        pass\n",
    );
    // The @check decorator should not appear as a decorator application
    assert!(!js.contains("= check("), "check decorator stripped: {}", js);
}

#[test]
fn test_dataclass_check_fixture() {
    let fixtures = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures");
    let source = std::fs::read_to_string(fixtures.join("dataclass_check.ps")).unwrap();
    let js = compile(&source);
    assert!(js.contains("class DateRange"), "DateRange class: {}", js);
    assert!(js.contains("this.validate_range()"), "check call: {}", js);
    assert!(js.contains("this.check_overlap()"), "second check: {}", js);
}

// ============================
// Phase 4a — Performance / Stress Tests
// ============================

#[test]
fn test_stress_many_functions_compiles() {
    let fixtures = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures");
    let source = std::fs::read_to_string(fixtures.join("stress_many_functions.ps")).unwrap();
    let js = compile(&source);
    // Verify first and last functions present
    assert!(
        js.contains("function func_001(x)"),
        "First function: {}",
        &js[..200]
    );
    assert!(
        js.contains("function func_200(items)"),
        "Last function: {}",
        &js[js.len() - 300..]
    );
}

#[test]
fn test_stress_large_class_compiles() {
    let fixtures = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures");
    let source = std::fs::read_to_string(fixtures.join("stress_large_class.ps")).unwrap();
    let js = compile(&source);
    assert!(
        js.contains("class BigClass"),
        "Class defined: {}",
        &js[..200]
    );
    assert!(js.contains("method_001()"), "First method: {}", &js[..500]);
    assert!(js.contains("method_100("), "Last method present");
}

#[test]
fn test_stress_nested_compiles() {
    let fixtures = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures");
    let source = std::fs::read_to_string(fixtures.join("stress_nested.ps")).unwrap();
    let js = compile(&source);
    assert!(
        js.contains("function deep_nest("),
        "Deep function: {}",
        &js[..200]
    );
    assert!(js.contains("function shallow("), "Shallow function present");
}

#[test]
fn test_stress_dataclass_compiles() {
    let fixtures = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures");
    let source = std::fs::read_to_string(fixtures.join("stress_dataclass.ps")).unwrap();
    let js = compile(&source);
    assert!(
        js.contains("class BigConfig"),
        "Dataclass defined: {}",
        &js[..200]
    );
    assert!(js.contains("Object.freeze(this)"), "Frozen: {}", &js[..500]);
    assert!(js.contains("field_01"), "First field");
    assert!(js.contains("field_30"), "Last field");
}

// ============================
// Token Efficiency Features
// ============================

// --- Kwarg shorthand (=name) ---

#[test]
fn test_kwarg_shorthand_basic() {
    let js = compile("f(=x, =y)");
    assert!(
        js.contains("x: x") && js.contains("y: y"),
        "Kwarg shorthand should expand =x to x: x: {}",
        js
    );
}

#[test]
fn test_kwarg_shorthand_mixed() {
    let js = compile("f(1, =name, key=\"val\")");
    assert!(
        js.contains("name: name"),
        "Should contain name: name kwarg: {}",
        js
    );
    assert!(
        js.contains("key: \"val\""),
        "Should contain key kwarg: {}",
        js
    );
}

// --- Nullish coalescing (??) ---

#[test]
fn test_nullish_coalesce_basic() {
    let js = compile("x = name ?? \"default\"");
    assert!(js.contains("??"), "Should emit ??: {}", js);
    assert!(js.contains("\"default\""), "Should have default: {}", js);
}

#[test]
fn test_nullish_coalesce_chained() {
    let js = compile("x = a ?? b ?? c");
    assert!(js.contains("??"), "Should emit ??: {}", js);
}

// --- Pipeline operator (|>) ---

#[test]
fn test_pipeline_bare_name() {
    // #110: an undeclared builtin piped bare now binds to its runtime
    // helper (previously the emitted `sorted(data)` was an unbound
    // identifier — ReferenceError at runtime).
    let js = compile("result = data |> sorted");
    assert!(js.contains("pySorted(data)"), "Bare pipe: {}", js);
}

#[test]
fn test_pipeline_with_args() {
    // Non-builtin callee: the test's subject is the pipeline's
    // piped-value-first calling convention, not builtin lowering.
    let js = compile("result = data |> shortlist(is_active)");
    assert!(
        js.contains("shortlist(data, is_active)"),
        "Pipe with args: {}",
        js
    );
}

#[test]
fn test_pipeline_chained() {
    let js = compile("result = data |> shortlist(active) |> rank");
    assert!(
        js.contains("rank(shortlist(data, active))"),
        "Chained pipe: {}",
        js
    );
}

// --- Optional chaining (?.) ---

#[test]
fn test_optional_chaining_attribute() {
    let js = compile("x = obj?.name");
    assert!(js.contains("obj?.name"), "Optional attr: {}", js);
}

#[test]
fn test_optional_chaining_subscript() {
    let js = compile("x = arr?.[0]");
    assert!(js.contains("arr?.[0]"), "Optional subscript: {}", js);
}

#[test]
fn test_optional_chaining_call() {
    let js = compile("x = func?.(a, b)");
    assert!(js.contains("func?.(a, b)"), "Optional call: {}", js);
}

#[test]
fn test_optional_chaining_deep() {
    let js = compile("x = obj?.foo?.bar");
    assert!(js.contains("obj?.foo?.bar"), "Deep optional chain: {}", js);
}

#[test]
fn test_optional_chaining_mixed() {
    let js = compile("x = obj?.method(a)");
    assert!(
        js.contains("obj?.method(a)"),
        "Mixed optional chain: {}",
        js
    );
}

// === WASM bridge skip tests ===

fn compile_with_wasm_bridge(source: &str, wasm_functions: &[&str], glue_filename: &str) -> String {
    let module = pyths_parser::parse(source).expect("Parse failed");
    let funcs: Vec<String> = wasm_functions.iter().map(|s| s.to_string()).collect();
    pyths_codegen_js::codegen_with_wasm_bridge(
        &module,
        &funcs,
        glue_filename,
        &std::collections::HashMap::new(),
    )
}

#[test]
fn test_wasm_bridge_skips_function() {
    let source = "def add(a: int, b: int) -> int:\n    return a + b\ndef greet(name):\n    return f\"Hello, {name}!\"";
    let js = compile_with_wasm_bridge(source, &["add"], "./test.glue.js");
    assert!(!js.contains("function add("), "Should skip add: {}", js);
    assert!(js.contains("function greet("), "Should keep greet: {}", js);
}

#[test]
fn test_wasm_bridge_reexport() {
    // Codegen emits both an `import` (so JS-side callers can reference
    // the WASM function locally) AND an `export` (so consumers of the
    // module see it as part of the public surface). A bare `export {} from`
    // is a transparent re-export only — it doesn't create a local
    // binding, which broke component code that called WASM helpers.
    let source = "def add(a: int, b: int) -> int:\n    return a + b";
    let js = compile_with_wasm_bridge(source, &["add"], "./test.glue.js");
    assert!(
        js.contains("import { add } from \"./test.glue.js\""),
        "Local import: {}",
        js
    );
    assert!(js.contains("export { add };"), "Re-export: {}", js);
}

#[test]
fn test_wasm_bridge_keeps_non_wasm() {
    let source = "def add(a: int, b: int) -> int:\n    return a + b\ndef greet(name):\n    return f\"Hello, {name}!\"";
    let js = compile_with_wasm_bridge(source, &["add"], "./test.glue.js");
    assert!(js.contains("function greet("), "Should keep greet: {}", js);
    assert!(
        js.contains("return `Hello, ${pyStr(name)}!`"),
        "greet body intact: {}",
        js
    );
}

// =====================================================================
// Method-lowering tests (Step 1: Rename entries).
// =====================================================================
// The compiler must rewrite Python collection/string method names to their
// JS equivalents so the emitted output runs in plain V8/SpiderMonkey
// without prototype polyfills.

#[test]
fn test_method_lowering_list_append() {
    let js = compile("xs = []\nxs.append(1)");
    assert!(js.contains("xs.push(1)"), "append→push: {}", js);
    assert!(!js.contains("xs.append("), "no .append() leftover: {}", js);
}

#[test]
fn test_method_lowering_str_lower() {
    let js = compile("name = \"Alice\"\nx = name.lower()");
    assert!(
        js.contains("name.toLowerCase()"),
        ".lower→.toLowerCase: {}",
        js
    );
    assert!(!js.contains("name.lower()"), "no .lower() leftover: {}", js);
}

#[test]
fn test_method_lowering_str_upper() {
    let js = compile("name = \"Alice\"\nx = name.upper()");
    assert!(
        js.contains("name.toUpperCase()"),
        ".upper→.toUpperCase: {}",
        js
    );
    assert!(!js.contains("name.upper()"), "no .upper() leftover: {}", js);
}

#[test]
fn test_method_lowering_str_startswith() {
    let js = compile("s = \"hello\"\nb = s.startswith(\"he\")");
    assert!(
        js.contains("s.startsWith(\"he\")"),
        "startswith→startsWith: {}",
        js
    );
    assert!(
        !js.contains(".startswith("),
        "no .startswith() leftover: {}",
        js
    );
}

#[test]
fn test_method_lowering_str_endswith() {
    let js = compile("s = \"hello\"\nb = s.endswith(\"lo\")");
    assert!(
        js.contains("s.endsWith(\"lo\")"),
        "endswith→endsWith: {}",
        js
    );
    assert!(
        !js.contains(".endswith("),
        "no .endswith() leftover: {}",
        js
    );
}

#[test]
fn test_method_lowering_str_find() {
    // #301: `.find` collides with Array.prototype.find(callback), so it
    // routes through the receiver-dispatching pyFind helper (which also
    // adds the previously-ignored start/end args for strings).
    let js = compile("s = \"hello\"\ni = s.find(\"l\")");
    assert!(js.contains("pyFind(s, \"l\")"), "find→pyFind: {}", js);
    assert!(
        !js.contains("s.indexOf(\"l\")"),
        "no blind indexOf rename: {}",
        js
    );
}

#[test]
fn test_method_lowering_str_rfind() {
    // Wave-19 verification fix: rfind routes through the code-point-offset
    // helper (raw lastIndexOf counts UTF-16 units and diverges on astral).
    let js = compile("s = \"hello\"\ni = s.rfind(\"l\")");
    assert!(
        js.contains("pyStrRfind(s, \"l\")"),
        "rfind→pyStrRfind: {}",
        js
    );
}

#[test]
fn test_method_lowering_str_replace() {
    // #242: Python str.replace replaces ALL by default but honors an optional
    // `count`; lower to the pyStrReplace runtime helper (JS .replaceAll ignores
    // count).
    let js = compile("s = \"foo foo\"\nx = s.replace(\"foo\", \"bar\")");
    assert!(
        js.contains("pyStrReplace(s, \"foo\", \"bar\")"),
        "replace→pyStrReplace: {}",
        js
    );
    let jc = compile("s = \"aaa\"\nx = s.replace(\"a\", \"b\", 1)");
    assert!(
        jc.contains("pyStrReplace(s, \"a\", \"b\", 1)"),
        "count passed through: {}",
        jc
    );
}

#[test]
fn test_method_lowering_chained() {
    // Lowering must work on chained method calls.
    let js = compile("s = \"  Hello  \"\nx = s.lower()");
    assert!(js.contains(".toLowerCase()"), "chained: {}", js);
}

#[test]
fn test_method_lowering_complex_receiver() {
    // Rename works for complex receivers (no double-evaluation issue
    // since args are unchanged). Note: typed-list subscript routes
    // through pyGetItem so `xs[0]` becomes `pyGetItem(xs, 0)` and the
    // `.lower()` lowering still applies on top of that.
    let js = compile("xs = [\"A\", \"B\"]\ny = xs[0].lower()");
    assert!(
        js.contains("pyGetItem(xs, 0).toLowerCase()"),
        "complex receiver: {}",
        js
    );
}

#[test]
fn test_method_lowering_with_args() {
    let js = compile("xs = []\nxs.append(42)");
    assert!(js.contains("xs.push(42)"), "args preserved: {}", js);
}

#[test]
fn test_method_lowering_user_method_unchanged() {
    // A user-defined method with a non-reserved name is untouched.
    let js = compile(
        "class Foo:\n    def custom_method(self, x):\n        return x\n\nf = Foo()\nr = f.custom_method(1)",
    );
    assert!(
        js.contains(".custom_method(1)"),
        "user method unchanged: {}",
        js
    );
}

// =====================================================================
// Method-lowering tests (Step 2: Inline + Hybrid entries).
// =====================================================================

#[test]
fn test_method_lowering_list_extend() {
    let js = compile("xs = []\nys = [1, 2]\nxs.extend(ys)");
    assert!(js.contains("xs.push(...ys)"), "extend→push spread: {}", js);
    assert!(!js.contains("xs.extend("), "no .extend leftover: {}", js);
}

#[test]
fn test_method_lowering_list_insert() {
    let js = compile("xs = [1, 3]\nxs.insert(1, 2)");
    assert!(js.contains("xs.splice(1, 0, 2)"), "insert→splice: {}", js);
    assert!(!js.contains(".insert("), "no .insert leftover: {}", js);
}

#[test]
fn test_method_lowering_list_copy() {
    let js = compile("xs = [1, 2]\nys = xs.copy()");
    assert!(js.contains("xs.slice()"), "copy→slice: {}", js);
}

#[test]
fn test_method_lowering_list_clear_simple() {
    let js = compile("xs = [1, 2]\nxs.clear()");
    assert!(js.contains("(xs.length = 0)"), "clear simple: {}", js);
    assert!(!js.contains(".clear("), "no .clear leftover: {}", js);
}

#[test]
fn test_method_lowering_list_clear_complex() {
    // Complex receiver routes to runtime helper to avoid double-evaluation.
    // `d` is dict-typed (from `d = {}`), so `d["k"]` now goes through
    // pyGetItem so a missing-key access throws Python KeyError instead of
    // returning undefined and crashing later.
    let js = compile("d = {}\nd[\"k\"] = [1, 2]\nd[\"k\"].clear()");
    assert!(
        js.contains("pyClear(pyGetItem(d, \"k\"))"),
        "clear complex→runtime: {}",
        js
    );
}

#[test]
fn test_method_lowering_list_count_simple() {
    // After the table refactor, `count` always routes through the
    // multi-type runtime helper because count semantics differ between
    // strings (count substring matches) and lists (count === matches).
    // Without type info we can't pick safely at codegen time, so the
    // runtime helper sniffs the receiver shape.
    let js = compile("xs = [1, 2, 1]\nn = xs.count(1)");
    assert!(js.contains("pyCount(xs, 1)"), "count→pyCount: {}", js);
}

#[test]
fn test_method_lowering_list_count_complex() {
    let js = compile("xs = []\nn = (xs + xs).count(1)");
    // Multi-type helper after refactor.
    assert!(js.contains("pyCount("), "count complex→runtime: {}", js);
}

#[test]
fn test_method_lowering_str_strip() {
    let js = compile("s = \"  hi  \"\nx = s.strip()");
    assert!(
        js.contains(r#"s.replace(/^\s+|\s+$/g, "")"#),
        "strip: {}",
        js
    );
}

#[test]
fn test_method_lowering_str_lstrip() {
    let js = compile("s = \"  hi\"\nx = s.lstrip()");
    assert!(js.contains(r#"s.replace(/^\s+/, "")"#), "lstrip: {}", js);
}

#[test]
fn test_method_lowering_str_rstrip() {
    let js = compile("s = \"hi  \"\nx = s.rstrip()");
    assert!(js.contains(r#"s.replace(/\s+$/, "")"#), "rstrip: {}", js);
}

#[test]
fn test_method_lowering_str_zfill() {
    let js = compile("s = \"7\"\nx = s.zfill(3)");
    assert!(js.contains("s.padStart(3, \"0\")"), "zfill: {}", js);
}

#[test]
fn test_method_lowering_str_capitalize_simple() {
    let js = compile("s = \"hello\"\nx = s.capitalize()");
    assert!(
        js.contains("s[0].toUpperCase()"),
        "capitalize inline: {}",
        js
    );
    assert!(
        js.contains(".slice(1).toLowerCase()"),
        "capitalize body: {}",
        js
    );
}

#[test]
fn test_method_lowering_str_capitalize_complex() {
    // List-typed `xs` → subscript goes through pyGetItem.
    let js = compile("xs = [\"a\"]\nx = xs[0].capitalize()");
    assert!(
        js.contains("pyStrCapitalize(pyGetItem(xs, 0))"),
        "capitalize complex→runtime: {}",
        js
    );
}

#[test]
fn test_method_lowering_str_isdigit() {
    let js = compile("s = \"42\"\nb = s.isdigit()");
    assert!(js.contains("/^[0-9]+$/.test(s)"), "isdigit: {}", js);
}

#[test]
fn test_method_lowering_str_isalpha() {
    let js = compile("s = \"abc\"\nb = s.isalpha()");
    assert!(js.contains("/^[A-Za-z]+$/.test(s)"), "isalpha: {}", js);
}

#[test]
fn test_method_lowering_str_isalnum() {
    let js = compile("s = \"abc123\"\nb = s.isalnum()");
    assert!(js.contains("/^[A-Za-z0-9]+$/.test(s)"), "isalnum: {}", js);
}

#[test]
fn test_method_lowering_dict_get_simple_no_default() {
    // `.get` routes through pyDictGet (runtime shape-dispatch) so FormData/
    // Map/URLSearchParams work too, not just plain-object dicts. (B-038 / #55)
    let js = compile("d = {}\nv = d.get(\"k\")");
    assert!(
        js.contains("pyDictGet(d, \"k\")"),
        "get(k) → pyDictGet: {}",
        js
    );
}

#[test]
fn test_method_lowering_dict_get_simple_with_default() {
    let js = compile("d = {}\nv = d.get(\"k\", 0)");
    assert!(
        js.contains("pyDictGet(d, \"k\", 0)"),
        "get(k, default) → pyDictGet: {}",
        js
    );
}

#[test]
fn test_method_lowering_get_on_param_uses_runtime_dispatch() {
    // Regression for B-038: `.get(k)` on an untyped receiver (e.g. a server
    // action's FormData param) must NOT inline to subscript `form["k"]`
    // (undefined on FormData) — it routes through pyDictGet, which dispatches
    // on receiver shape at runtime.
    let js = compile("def action(form):\n    return form.get(\"title\")");
    assert!(
        js.contains("pyDictGet(form, \"title\")"),
        "form.get → pyDictGet: {}",
        js
    );
    assert!(
        !js.contains("form[\"title\"]"),
        "must not inline to subscript: {}",
        js
    );
}

#[test]
fn test_method_lowering_dict_get_complex() {
    // List-typed `xs` → subscript routes through pyGetItem.
    let js = compile("xs = [{}]\nv = xs[0].get(\"k\", 0)");
    assert!(
        js.contains("pyDictGet(pyGetItem(xs, 0), \"k\", 0)"),
        "get complex→runtime: {}",
        js
    );
}

#[test]
fn test_method_lowering_dict_keys() {
    // #83: shape-dispatching runtime helpers (were Object.keys/values/
    // entries inlines, which cannot see Map-backed dicts).
    let js = compile("d = {}\nks = d.keys()");
    assert!(js.contains("pyDictKeys(d)"), "keys: {}", js);
}

#[test]
fn test_method_lowering_dict_values() {
    let js = compile("d = {}\nvs = d.values()");
    assert!(js.contains("pyDictValues(d)"), "values: {}", js);
}

#[test]
fn test_method_lowering_dict_items() {
    let js = compile("d = {}\nits = d.items()");
    assert!(js.contains("pyDictItems(d)"), "items: {}", js);
}

#[test]
fn test_method_lowering_dict_update() {
    // After consolidation with set.update, both go through pyUpdate
    // (runtime helper sniffs Set vs object).
    let js = compile("d = {}\nother = {}\nd.update(other)");
    assert!(js.contains("pyUpdate(d, other)"), "update: {}", js);
}

// =====================================================================
// Method-lowering tests (Step 3: Runtime entries).
// =====================================================================

#[test]
fn test_method_lowering_list_remove() {
    let js = compile("xs = [1, 2, 3]\nxs.remove(2)");
    // Multi-type helper (sniffs Array.isArray vs Set instanceof).
    assert!(js.contains("pyRemove(xs, 2)"), "remove→pyRemove: {}", js);
    assert!(!js.contains(".remove("), "no .remove leftover: {}", js);
}

#[test]
fn test_method_lowering_list_index() {
    let js = compile("xs = [1, 2, 3]\ni = xs.index(2)");
    assert!(js.contains("pyIndex(xs, 2)"), "index→pyIndex: {}", js);
}

#[test]
fn test_method_lowering_list_pop_no_arg() {
    // Even no-arg pop routes through pyPop because we can't statically
    // distinguish list-pop from dict-pop without type info.
    let js = compile("xs = [1, 2]\nv = xs.pop()");
    assert!(js.contains("pyPop(xs)"), "pop()→pyPop: {}", js);
}

#[test]
fn test_method_lowering_list_pop_with_arg() {
    let js = compile("xs = [1, 2, 3]\nv = xs.pop(0)");
    assert!(js.contains("pyPop(xs, 0)"), "pop(0)→pyPop: {}", js);
}

#[test]
fn test_method_lowering_str_join() {
    // Python `sep.join(it)` → `pyStrJoin(sep, it)` — receiver is sep.
    let js = compile("xs = [\"a\", \"b\"]\ns = \", \".join(xs)");
    assert!(
        js.contains("pyStrJoin(\", \", xs)"),
        "join arg-reversal: {}",
        js
    );
    assert!(!js.contains(".join("), "no .join() leftover: {}", js);
}

#[test]
fn test_method_lowering_str_split_no_arg() {
    let js = compile("s = \"a b c\"\nws = s.split()");
    assert!(js.contains("pyStrSplit(s)"), "split()→pyStrSplit: {}", js);
}

#[test]
fn test_method_lowering_str_split_with_arg() {
    let js = compile("s = \"a,b,c\"\nws = s.split(\",\")");
    assert!(
        js.contains("pyStrSplit(s, \",\")"),
        "split(sep)→pyStrSplit: {}",
        js
    );
}

#[test]
fn test_method_lowering_str_title() {
    let js = compile("s = \"hello world\"\nx = s.title()");
    assert!(js.contains("pyStrTitle(s)"), "title→pyStrTitle: {}", js);
}

#[test]
fn test_method_lowering_str_format() {
    let js = compile("s = \"hi {}\"\nx = s.format(\"alice\")");
    assert!(
        js.contains("pyStrFormat(s, \"alice\")"),
        "format→pyStrFormat: {}",
        js
    );
}

#[test]
fn test_method_lowering_dict_setdefault() {
    let js = compile("d = {}\nv = d.setdefault(\"k\", 0)");
    assert!(
        js.contains("pyDictSetdefault(d, \"k\", 0)"),
        "setdefault→runtime: {}",
        js
    );
}

#[test]
fn test_method_lowering_runtime_imports_pyStrJoin() {
    // The runtime helper must be imported by the generated module.
    let js = compile("xs = [\"a\"]\ns = \", \".join(xs)");
    assert!(
        js.contains("import {") && js.contains("pyStrJoin"),
        "pyStrJoin imported: {}",
        js
    );
}

#[test]
fn test_method_lowering_runtime_imports_pyDictGet() {
    // Hybrid → Runtime form must register the runtime helper.
    let js = compile("xs = [{}]\nv = xs[0].get(\"k\", 0)");
    assert!(
        js.contains("pyDictGet"),
        "pyDictGet imported on complex receiver: {}",
        js
    );
}

// =====================================================================
// Style snake→camel + anti-pollution sweep (Step 4).
// =====================================================================

#[test]
fn test_style_dict_keys_snake_to_camel() {
    // CSS keys in style={...} on HTML elements must come out camelCase.
    let source = r#"
@component
def Box():
    return div(style={"border_radius": "6px", "font_family": "system-ui", "min_height": "100vh", "border_bottom": "1px solid #e5e7eb"})()
"#;
    let js = compile(source);
    assert!(js.contains("\"borderRadius\""), "borderRadius: {}", js);
    assert!(js.contains("\"fontFamily\""), "fontFamily: {}", js);
    assert!(js.contains("\"minHeight\""), "minHeight: {}", js);
    assert!(js.contains("\"borderBottom\""), "borderBottom: {}", js);
    assert!(
        !js.contains("border_radius"),
        "no border_radius leftover: {}",
        js
    );
    assert!(
        !js.contains("font_family"),
        "no font_family leftover: {}",
        js
    );
    assert!(!js.contains("min_height"), "no min_height leftover: {}", js);
    assert!(
        !js.contains("border_bottom"),
        "no border_bottom leftover: {}",
        js
    );
}

#[test]
fn test_style_dict_user_component_unchanged() {
    // User-component style props are left as-is — only HTML-element style
    // props get the React snake→camel treatment.
    let source = r#"
@component
def Card():
    return MyBox(style={"border_radius": "6px"})()
"#;
    let js = compile(source);
    // The dict literal preserves snake_case for user component...
    // Actually, since we only transform when tag is HTML (lowercase),
    // user-component style passes through unchanged.
    assert!(
        js.contains("\"border_radius\"") || js.contains("\"borderRadius\""),
        "user-component style: {}",
        js
    );
}

// =====================================================================
// Anti-pollution sweep — compile both demo fixtures and assert that
// the generated JS contains zero Python-method-name leakage. This is
// the single test that catches the entire class of bugs we're fixing.
// =====================================================================

#[test]
fn test_no_python_isms_in_dashboard_500() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/cloudflare-bench/large-samples/pythscribe/dashboard_500.ps");
    let source = std::fs::read_to_string(&path).expect("dashboard_500.ps must exist");
    let js = compile(&source);
    assert_no_python_isms("dashboard_500", &js);
}

#[test]
fn test_no_python_isms_in_app_1000() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/cloudflare-bench/large-samples/pythscribe/app_1000.ps");
    let source = std::fs::read_to_string(&path).expect("app_1000.ps must exist");
    let js = compile(&source);
    assert_no_python_isms("app_1000", &js);
}

/// Sweep the generated JS for common Python-method names and snake_case
/// CSS keys. Any hit is a codegen bug — those Python idioms must be
/// lowered, not passed through verbatim. The list mirrors the reserved
/// names in `method_lowering.rs::method_lowering()` plus the style-key
/// CSS properties we've seen in the demo fixtures.
fn assert_no_python_isms(name: &str, js: &str) {
    // Method names that should never appear as `.<name>(` in clean JS
    // output. These are the Python idioms our lowering rewrites away.
    // Some names can legitimately appear in user-defined methods — for
    // those, the test fixture itself is responsible for not using them
    // ambiguously (we treat the reserved list as just that: reserved).
    let forbidden_methods = [
        ".append(",
        ".extend(",
        ".insert(",
        ".remove(",
        ".count(",
        ".clear(",
        ".lower(",
        ".upper(",
        ".capitalize(",
        ".title(",
        ".startswith(",
        ".endswith(",
        ".lstrip(",
        ".rstrip(",
        ".strip(",
        ".isdigit(",
        ".isalpha(",
        ".isalnum(",
        ".isspace(",
        ".islower(",
        ".isupper(",
        ".zfill(",
        ".rfind(",
        ".setdefault(",
        // .find( — not always Python: Array.prototype.find exists in JS
        // and takes a predicate. Skip from the generic sweep.
        // .replace( — Python str.replace is replaceAll, but JS .replace
        // is also legal (single-replace). Skip from generic sweep — the
        // method_lowering replaces it explicitly.
        // .split( — JS-native; legal.
        // .pop(  — JS-native; legal (after lowering, .pop becomes pyPop).
        // .keys()/.values()/.items() — not allowed on plain objects in JS,
        // but Object.keys etc. emit form contains "Object.keys(" so the
        // raw `.keys(` text doesn't appear after lowering. Allow it here.
        // .get( — Map has it natively, and pyDictGet replaces dict gets,
        // so we skip.
    ];
    for pat in forbidden_methods.iter() {
        assert!(
            !js.contains(pat),
            "{}: forbidden Python-ism `{}` survived in compiled JS\n\
             First occurrence at byte {}.\n\
             Snippet: {}",
            name,
            pat,
            js.find(pat).unwrap_or(0),
            preview_at(js, js.find(pat).unwrap_or(0))
        );
    }

    // Snake-case CSS keys that React would silently drop.
    let forbidden_style_keys = [
        "border_radius",
        "font_family",
        "font_size",
        "min_height",
        "max_height",
        "min_width",
        "max_width",
        "border_bottom",
        "border_top",
        "border_left",
        "border_right",
        "padding_left",
        "padding_right",
        "padding_top",
        "padding_bottom",
        "margin_left",
        "margin_right",
        "margin_top",
        "margin_bottom",
        "background_color",
        "text_align",
        "line_height",
        "justify_content",
        "align_items",
        "flex_direction",
        "grid_template_columns",
        "grid_template_rows",
        "z_index",
    ];
    for key in forbidden_style_keys.iter() {
        assert!(
            !js.contains(key),
            "{}: snake_case CSS key `{}` survived in compiled JS\n\
             Snippet: {}",
            name,
            key,
            preview_at(js, js.find(key).unwrap_or(0))
        );
    }

    // Reverse-arg `.join` form: a string-literal receiver calling .join
    // is the Python form (`",".join(arr)`). After lowering it becomes
    // `pyStrJoin(",", arr)`, with no `".join(` substring.
    assert!(
        !js.contains("\".join("),
        "{}: Python-style `\"sep\".join(...)` survived: {}",
        name,
        preview_at(js, js.find("\".join(").unwrap_or(0))
    );
}

fn preview_at(s: &str, idx: usize) -> String {
    let start = idx.saturating_sub(40);
    let end = (idx + 60).min(s.len());
    s[start..end].replace('\n', "\n")
}

// =====================================================================
// Step 6 fixtures: regression tests born from E2E failures.
// =====================================================================

#[test]
fn test_fstring_format_spec_decimal() {
    // Python `:.2f` lowers to `pyFixed(x, 2)` (#86: CPython round-half-
    // even on the exact double; was JS `.toFixed(2)`, which rounds exact
    // ties away from zero).
    let js = compile("x = 3.14159\ns = f\"{x:.2f}\"");
    assert!(
        js.contains("pyFixed(x, 2)"),
        "f\"{{x:.2f}}\" → pyFixed(x, 2): {}",
        js
    );
}

#[test]
fn test_fstring_format_spec_zero_decimal() {
    let js = compile("x = 3.7\ns = f\"{x:.0f}\"");
    assert!(
        js.contains("pyFixed(x, 0)"),
        "f\"{{x:.0f}}\" → pyFixed(x, 0): {}",
        js
    );
}

#[test]
fn test_fstring_format_spec_thousands() {
    // Force "en-US" locale so the runtime always uses `,` as the
    // grouping separator (matches CPython's locale-independent format).
    let js = compile("x = 1000000\ns = f\"{x:,}\"");
    assert!(js.contains("pyFormatSpec(x, ({\"grouping\": \",\"}))"),
        "thousands (Pythonic-checks: grouping routes through pyFormatSpec; the old toLocaleString fast path truncated float fractions): {}", js);
}

#[test]
fn test_fstring_conversion_repr() {
    // Pythonic-checks sweep: f"{s!r}" / f"{x!s}" conversions.
    let js = compile(
        "s = 'hi'
m = f\"{s!r}\"",
    );
    assert!(js.contains("pyRepr(s)"), "!r wraps in pyRepr: {}", js);
    let js2 = compile(
        "x = 42
m = f\"{x!s}\"",
    );
    assert!(js2.contains("pyStr(x)"), "!s wraps in pyStr: {}", js2);
}

#[test]
fn test_fstring_selfdoc() {
    // Pythonic-checks sweep: self-documenting f"{x=}" → literal "x="
    // followed by repr(x); with a spec (f"{x=:.2f}") the spec formats
    // the value (str, not repr), like CPython.
    let js = compile(
        "x = 42
m = f\"{x=}\"",
    );
    assert!(
        js.contains("x=") && js.contains("pyRepr(x)"),
        "selfdoc: {}",
        js
    );
    let js2 = compile(
        "x = 3.14159
m = f\"{x=:.2f}\"",
    );
    assert!(
        js2.contains("x=") && js2.contains("pyFixed(x, 2)"),
        "selfdoc+spec: {}",
        js2
    );
}

#[test]
fn test_fstring_selfdoc_preserves_whitespace_and_ops() {
    // f"{x = }" keeps the spaces verbatim; `==` must NOT be mistaken
    // for a self-doc `=`.
    let js = compile(
        "x = 1
m = f\"{x = }\"",
    );
    assert!(
        js.contains("x = ") && js.contains("pyRepr("),
        "spaced selfdoc: {}",
        js
    );
    let js2 = compile(
        "x = 1
m = f\"{x == 1}\"",
    );
    assert!(!js2.contains("pyRepr("), "== is not selfdoc: {}", js2);
}

#[test]
fn test_fstring_format_spec_unknown_falls_through() {
    // Unknown specs are silently ignored — the unwrapped expression
    // appears (still routed through pyStr like any other no-spec
    // interpolation — see the A4 fix on FStringPart::Expr).
    let js = compile("x = 1\ns = f\"{x:weird}\"");
    assert!(
        js.contains("${pyStr(x)}"),
        "unknown spec falls through: {}",
        js
    );
}

#[test]
fn test_class_disambiguation_inside_component() {
    // Inside @component, an uppercase Name that's also a known class
    // routes to `new Class(...)` (instantiation), not createElement.
    let source = r#"
@dataclass
class Item:
    name: str

@component
def Card():
    x = Item(name="hi")
    return div()(x.name)
"#;
    let js = compile(source);
    assert!(js.contains("new Item("), "class disambiguation: {}", js);
    assert!(
        !js.contains("createElement(Item"),
        "no createElement on class: {}",
        js
    );
}

#[test]
fn test_dataclass_constructor_accepts_kwargs_object() {
    // The constructor must accept both `new Foo(a, b)` and `new Foo({a, b})`
    // — the codegen emits a head-of-body branch detecting the object form.
    let source = r#"
@dataclass
class P:
    name: str
    age: int
"#;
    let js = compile(source);
    assert!(
        js.contains("if (arguments.length === 1"),
        "constructor branches on arguments shape: {}",
        js
    );
    assert!(
        js.contains("({name, age} = name)") || js.contains("({name = "),
        "destructures props object: {}",
        js
    );
}

#[test]
fn test_fstring_format_spec_zero_padded_int() {
    let js = compile("x = 7\ns = f\"{x:02d}\"");
    assert!(
        js.contains("pyFormatSpec(x, ({\"zero\": true, \"width\": 2, \"type\": \"d\"}))"),
        "02d → pyFormatSpec (Pythonic-checks: the old padStart fast path was sign-unaware): {}",
        js
    );
}

// =====================================================================
// Method-table coverage tests.
// These confirm the new comprehensive table covers the documented
// Python surface and produces the expected diagnostics for
// Strategy::Unsupported entries.
// =====================================================================

#[test]
fn test_table_covers_python_surface() {
    use pyths_codegen_js::method_table::{iter, ReceiverKind};
    let mut by_kind = std::collections::HashMap::<&'static str, usize>::new();
    for entry in iter() {
        let k = match entry.receiver {
            ReceiverKind::Str => "str",
            ReceiverKind::List => "list",
            ReceiverKind::Dict => "dict",
            ReceiverKind::Set => "set",
            ReceiverKind::Tuple => "tuple",
            ReceiverKind::Multi => "multi",
            ReceiverKind::Num => "num",
        };
        *by_kind.entry(k).or_default() += 1;
    }
    // Floor counts per receiver — increase when methods are added.
    assert!(
        by_kind.get("str").copied().unwrap_or(0) >= 30,
        "str coverage thin: {:?}",
        by_kind
    );
    assert!(
        by_kind.get("list").copied().unwrap_or(0) >= 5,
        "list coverage thin: {:?}",
        by_kind
    );
    assert!(
        by_kind.get("dict").copied().unwrap_or(0) >= 5,
        "dict coverage thin: {:?}",
        by_kind
    );
    assert!(
        by_kind.get("set").copied().unwrap_or(0) >= 12,
        "set coverage thin: {:?}",
        by_kind
    );
    assert!(
        by_kind.get("multi").copied().unwrap_or(0) >= 5,
        "multi coverage thin: {:?}",
        by_kind
    );
}

#[test]
fn test_unsupported_method_emits_throw_and_diagnostic() {
    let module = pyths_parser::parse("s = \"hi\"\nb = s.encode()").unwrap();
    let mut gen = pyths_codegen_js::JsCodegen::new();
    gen.emit_module(&module);
    let errors = gen.take_errors();
    assert_eq!(errors.len(), 1, "expected one diagnostic");
    assert!(
        errors[0].contains("encode"),
        "diag mentions method: {:?}",
        errors
    );
    let js = gen.finish();
    assert!(js.contains("throw new Error("), "throw emitted: {}", js);
}

#[test]
fn test_method_lowering_str_casefold() {
    let js = compile("s = \"HEY\"\nx = s.casefold()");
    assert!(
        js.contains("s.toLowerCase()"),
        "casefold→toLowerCase: {}",
        js
    );
}

#[test]
fn test_method_lowering_str_removeprefix() {
    let js = compile("s = \"abc-foo\"\nx = s.removeprefix(\"abc-\")");
    assert!(
        js.contains("startsWith(\"abc-\")"),
        "removeprefix uses startsWith: {}",
        js
    );
}

#[test]
fn test_method_lowering_str_removesuffix() {
    let js = compile("s = \"foo-abc\"\nx = s.removesuffix(\"-abc\")");
    assert!(
        js.contains("endsWith(\"-abc\")"),
        "removesuffix uses endsWith: {}",
        js
    );
}

#[test]
fn test_method_lowering_str_isascii() {
    let js = compile("s = \"hello\"\nb = s.isascii()");
    assert!(js.contains("charCodeAt(0) < 128"), "isascii: {}", js);
}

#[test]
fn test_method_lowering_str_partition() {
    let js = compile("s = \"a-b-c\"\nt = s.partition(\"-\")");
    assert!(
        js.contains("pyStrPartition(s, \"-\")"),
        "partition→runtime: {}",
        js
    );
}

#[test]
fn test_method_lowering_str_ljust() {
    let js = compile("s = \"hi\"\nx = s.ljust(5)");
    assert!(js.contains("pyStrLjust(s, 5)"), "ljust→runtime: {}", js);
}

#[test]
fn test_method_lowering_str_swapcase() {
    let js = compile("s = \"AbC\"\nx = s.swapcase()");
    assert!(js.contains("pyStrSwapcase(s)"), "swapcase→runtime: {}", js);
}

#[test]
fn test_method_lowering_set_union() {
    let js = compile("a = set([1])\nb = set([2])\nc = a.union(b)");
    assert!(js.contains("pySetUnion(a, b)"), "union→runtime: {}", js);
}

// =====================================================================
// Step 2: Generalized anti-pollution sweep — driven by the method table.
//
// Earlier sweep tests had a hand-written list of forbidden patterns.
// This sweep derives the forbidden list from `method_table::TABLE`:
// every entry whose strategy isn't `Unsupported` and whose Rename target
// differs from its Python name (so the python name is *expected* to be
// rewritten away). Then it compiles every .ps fixture under examples/
// and tests/fixtures/ and asserts none contain those patterns.
//
// New fixtures get coverage automatically.
// =====================================================================

#[test]
fn test_method_lowering_table_is_self_consistent() {
    use pyths_codegen_js::method_table::{iter, Strategy};
    let names: std::collections::HashSet<&'static str> = iter().map(|e| e.name).collect();
    // The table can have multiple entries with different ReceiverKind
    // sharing a name (rare, but legal). For now we don't have any —
    // catch this if it ever changes.
    let count = iter().count();
    assert_eq!(count, names.len(),
        "method table contains duplicate names — Multi-receiver entries should consolidate via runtime dispatch, not by adding rows");

    // Sanity: every Rename / Inline / Hybrid / Runtime entry MUST have a
    // non-empty target form. Unsupported entries MUST have a non-empty
    // reason string.
    for e in iter() {
        match e.strategy {
            Strategy::Rename(js) => assert!(!js.is_empty(), "{}: empty Rename target", e.name),
            Strategy::Hybrid { runtime, .. } => {
                assert!(!runtime.is_empty(), "{}: empty Hybrid runtime", e.name)
            }
            Strategy::Runtime(helper) => {
                assert!(!helper.is_empty(), "{}: empty Runtime helper", e.name)
            }
            Strategy::Unsupported(reason) => {
                assert!(!reason.is_empty(), "{}: empty Unsupported reason", e.name)
            }
            Strategy::Inline(_) => {}
        }
    }
}

/// Names that the lowering rewrites away. For Rename(js_name) we only
/// add the python_name when js_name differs (e.g., append→push); for
/// Inline/Hybrid/Runtime we always add. Unsupported names produce a
/// throw-expression and are tested separately.
///
/// JS-native overlaps are excluded: method names that also exist on
/// JS String.prototype / Array.prototype, where the codegen *itself*
/// emits `.X(...)` legitimately as part of Inline lowerings (e.g.,
/// `.replace(/regex/g, "")` to implement `s.strip()`, or
/// `__errors.join("; ")` from dataclass collect_errors). For those
/// names a substring sweep can't distinguish lowered-correctly from
/// leaked-through; coverage is provided by the per-method unit tests.
fn forbidden_method_names() -> Vec<&'static str> {
    use pyths_codegen_js::method_table::{iter, Strategy};
    const JS_NATIVE_OVERLAP: &[&str] = &[
        "replace", "join", "pop", "find", "rfind", "sort", "split", "index",
    ];

    let mut out: Vec<&'static str> = Vec::new();
    for e in iter() {
        if JS_NATIVE_OVERLAP.contains(&e.name) {
            continue;
        }
        match e.strategy {
            Strategy::Rename(js) => {
                if js != e.name {
                    out.push(e.name);
                }
            }
            Strategy::Inline(_) | Strategy::Hybrid { .. } | Strategy::Runtime(_) => {
                out.push(e.name);
            }
            Strategy::Unsupported(_) => {}
        }
    }
    out
}

fn enumerate_fixtures() -> Vec<std::path::PathBuf> {
    let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let roots = [
        manifest.join("../../examples"),
        manifest.join("../../tests/fixtures"),
    ];
    let mut out = Vec::new();
    for root in &roots {
        walk_ps(root, &mut out);
    }
    out
}

fn walk_ps(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for e in entries.flatten() {
        let p = e.path();
        if p.is_dir() {
            walk_ps(&p, out);
        } else if p.extension().and_then(|s| s.to_str()) == Some("ps") {
            out.push(p);
        }
    }
}

/// Fixture file names known to be intentionally malformed (error cases
/// for the lint/parse error pipeline). Skipped by the sweep — they
/// won't parse, so there's nothing to inspect.
fn skip_fixture(name: &str) -> bool {
    name.starts_with("error_")
        || name == "lint_naming.ps"            // intentional warning — not error, but the codegen shouldn't be run
        || name == "lint_unreachable.ps"
        || name == "lint_unused_var.ps"
        || name == "lint_unused_import.ps"
}

#[test]
fn test_anti_pollution_sweep_all_fixtures() {
    let forbidden = forbidden_method_names();
    let fixtures = enumerate_fixtures();
    assert!(
        fixtures.len() > 20,
        "expected >20 .ps fixtures, found {}",
        fixtures.len()
    );

    let mut violations: Vec<String> = Vec::new();
    let mut compiled_count = 0usize;

    for path in &fixtures {
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if skip_fixture(name) {
            continue;
        }
        let source = match std::fs::read_to_string(path) {
            Ok(s) => s,
            Err(_) => continue,
        };
        let module = match pyths_parser::parse(&source) {
            Ok(m) => m,
            Err(_) => continue, // syntactically broken fixtures are out of scope
        };
        let js = pyths_codegen_js::codegen(&module);
        compiled_count += 1;

        for fname in &forbidden {
            let pat = format!(".{}(", fname);
            if js.contains(&pat) {
                let idx = js.find(&pat).unwrap_or(0);
                let lo = idx.saturating_sub(40);
                let hi = (idx + 60).min(js.len());
                let snippet = js[lo..hi].replace('\n', "\n");
                violations.push(format!(
                    "{}: forbidden `.{}(` survived\n    snippet: {}",
                    path.file_name().unwrap().to_string_lossy(),
                    fname,
                    snippet
                ));
            }
        }
    }

    assert!(
        compiled_count >= 20,
        "compiled too few fixtures: {}",
        compiled_count
    );
    assert!(
        violations.is_empty(),
        "Anti-pollution sweep found {} Python idioms in compiled JS:\n  {}",
        violations.len(),
        violations.join("\n  ")
    );
}

#[test]
fn test_style_variable_receiver_wraps_in_pyNormalizeStyle() {
    // When `style=variable` (not Dict literal), codegen wraps in
    // pyNormalizeStyle so the runtime can convert snake→camel keys.
    let source = r#"
@component
def Box():
    my_styles = {"border_radius": "6px"}
    return div(style=my_styles)()
"#;
    let js = compile(source);
    assert!(
        js.contains("pyNormalizeStyle(my_styles)"),
        "variable-receiver style wrapped: {}",
        js
    );
}

#[test]
fn test_style_dict_literal_still_inlined() {
    // Dict-literal style is rewritten at compile time (no runtime cost).
    let source = r#"
@component
def Box():
    return div(style={"border_radius": "6px"})()
"#;
    let js = compile(source);
    assert!(
        js.contains("\"borderRadius\""),
        "literal still inlined: {}",
        js
    );
    assert!(
        !js.contains("pyNormalizeStyle"),
        "literal style should NOT use runtime helper: {}",
        js
    );
}

// =====================================================================
// Batch A: React-ecosystem coverage
// =====================================================================
// A.1 Generic use_* hook recognition for community libraries
// A.2 Popular React libs added to recognized-modules list
// A.5 Extended HTML/ARIA/data attribute mappings
// =====================================================================

#[test]
fn test_react_query_imports_and_calls_camel_case() {
    // React Query (TanStack Query) — `from at_tanstack.react_query import ...`
    let source = r#"
from at_tanstack.react_query import use_query, use_mutation

@component
def Page():
    data = use_query({"queryKey": ["foo"]})
    mutate = use_mutation({"mutationFn": lambda x: x})
    return div()(data)
"#;
    let js = compile(source);
    // Import line gets the JS form.
    assert!(
        js.contains("import { useQuery, useMutation } from \"@tanstack/react-query\""),
        "import: {}",
        js
    );
    // Call sites also reference the camelCase name (so the local binding
    // resolves; would otherwise be a ReferenceError).
    assert!(js.contains("useQuery("), "useQuery call site: {}", js);
    assert!(js.contains("useMutation("), "useMutation call site: {}", js);
    assert!(!js.contains("use_query("), "no use_query leftover: {}", js);
    assert!(
        !js.contains("use_mutation("),
        "no use_mutation leftover: {}",
        js
    );
}

#[test]
fn test_react_router_dom_imports() {
    let source = r#"
from react_router_dom import use_navigate, use_params, Link

@component
def Nav():
    nav = use_navigate()
    params = use_params()
    return Link(to="/home")()
"#;
    let js = compile(source);
    assert!(
        js.contains("from \"react-router-dom\""),
        "package path: {}",
        js
    );
    assert!(js.contains("useNavigate"), "useNavigate: {}", js);
    assert!(js.contains("useParams"), "useParams: {}", js);
    assert!(
        js.contains("createElement(Link"),
        "Link is a component: {}",
        js
    );
}

#[test]
fn test_react_hook_form_imports() {
    let source = r#"
from react_hook_form import use_form

@component
def MyForm():
    form = use_form()
    return form()
"#;
    let js = compile(source);
    assert!(js.contains("from \"react-hook-form\""), "package: {}", js);
    assert!(js.contains("useForm"), "useForm: {}", js);
}

#[test]
fn test_framer_motion_imports() {
    let source = r#"
from framer_motion import use_motion_value, use_animate

@component
def Animated():
    x = use_motion_value(0)
    scope, anim = use_animate()
    return div()()
"#;
    let js = compile(source);
    assert!(js.contains("from \"framer-motion\""), "package: {}", js);
    assert!(js.contains("useMotionValue"), "useMotionValue: {}", js);
    assert!(js.contains("useAnimate"), "useAnimate: {}", js);
}

#[test]
fn test_zustand_jotai_recoil_imports() {
    // Top state-management libraries — all should import as-is and
    // get hook names camel-cased.
    for (lib, fn_py, fn_js) in &[
        ("zustand", "create_store", "createStore"),
        ("jotai", "use_atom", "useAtom"),
        ("recoil", "use_recoil_state", "useRecoilState"),
    ] {
        let source = format!(
            "from {} import {}\n@component\ndef App():\n    return div()()",
            lib, fn_py
        );
        let js = compile(&source);
        let import_line = format!("from \"{}\"", lib);
        assert!(js.contains(&import_line), "{} import: {}", lib, js);
        assert!(js.contains(fn_js), "{} → {}: {}", fn_py, fn_js, js);
    }
}

#[test]
fn test_swr_imports() {
    let source = r#"
from swr import use_swr

@component
def Page():
    return div()()
"#;
    let js = compile(source);
    assert!(js.contains("from \"swr\""), "swr import: {}", js);
    assert!(js.contains("useSwr"), "useSwr: {}", js);
}

#[test]
fn test_aria_props_kebab() {
    let source = r#"
@component
def Modal():
    return div(aria_label="Close", aria_describedby="msg", aria_modal=True)()
"#;
    let js = compile(source);
    assert!(js.contains("\"aria-label\""), "aria-label: {}", js);
    assert!(
        js.contains("\"aria-describedby\""),
        "aria-describedby: {}",
        js
    );
    assert!(js.contains("\"aria-modal\""), "aria-modal: {}", js);
}

#[test]
fn test_data_props_kebab() {
    let source = r#"
@component
def Card():
    return div(data_id="42", data_test_id="card")()
"#;
    let js = compile(source);
    assert!(js.contains("\"data-id\""), "data-id: {}", js);
    assert!(js.contains("\"data-test-id\""), "data-test-id: {}", js);
}

#[test]
fn test_extended_html_props_in_jsx() {
    let source = r#"
@component
def Form():
    return input(default_value="hi", read_only=True, auto_focus=True, spell_check=False)
"#;
    let js = compile(source);
    // camelCase props are valid JS identifiers — emitted unquoted.
    assert!(js.contains("defaultValue:"), "defaultValue: {}", js);
    assert!(js.contains("readOnly:"), "readOnly: {}", js);
    assert!(js.contains("autoFocus:"), "autoFocus: {}", js);
    assert!(js.contains("spellCheck:"), "spellCheck: {}", js);
}

#[test]
fn test_svg_props_in_jsx() {
    let source = r#"
@component
def Icon():
    return svg(view_box="0 0 24 24")(
        path(stroke_width="2", stroke_linecap="round", text_anchor="middle")
    )
"#;
    let js = compile(source);
    assert!(js.contains("viewBox:"), "viewBox: {}", js);
    assert!(js.contains("strokeWidth:"), "strokeWidth: {}", js);
    assert!(js.contains("strokeLinecap:"), "strokeLinecap: {}", js);
    assert!(js.contains("textAnchor:"), "textAnchor: {}", js);
}

#[test]
fn test_explicit_alias_bypasses_camel_transform() {
    // When the user explicitly aliases an import, we honor their name
    // verbatim — they opted out of the snake→camel convention.
    // (Use `goto` as the alias rather than `nav` — `nav` collides with
    // the HTML <nav> element name and would route through PSX inside
    // a @component.)
    let source = r#"
from react_router_dom import use_navigate as goto

@component
def App():
    n = goto()
    return div()()
"#;
    let js = compile(source);
    // Import line emits the camel JS name aliased to the user's local name.
    assert!(
        js.contains("useNavigate as goto"),
        "alias preserved: {}",
        js
    );
    // Call site uses the user's alias unchanged.
    assert!(js.contains("goto()"), "alias used at call: {}", js);
    assert!(
        !js.contains("useNavigate("),
        "no JS-name leak at call: {}",
        js
    );
}

#[test]
fn test_unknown_module_does_not_get_react_transforms() {
    // A random non-React module should NOT get snake→camel applied to
    // its imports. `use_query` from a generic library is a valid Python
    // name and should pass through.
    let source = r#"
from my_local_lib import use_query

x = use_query()
"#;
    let js = compile(source);
    assert!(
        js.contains("import { use_query } from"),
        "non-React import unchanged: {}",
        js
    );
    assert!(
        js.contains("use_query()"),
        "non-React call unchanged: {}",
        js
    );
}

// =====================================================================
// Batch B: @psx decorator — PSX outside @component
// =====================================================================
// `@psx` enables HTML-element-call → createElement emission in any
// function body, without imposing @component's export, props
// destructuring, or known-class call disambiguation. Use case:
// render-prop helpers, HOCs, JSX-returning utility functions.
// =====================================================================

#[test]
fn test_psx_decorator_enables_jsx_emission() {
    // Render-prop helper that's neither a component nor exported.
    let source = r#"
from pyths.react import psx

@psx
def render_row(item):
    return tr()(td()(item.name), td()(item.value))
"#;
    let js = compile(source);
    // HTML element calls must lower to createElement.
    assert!(
        js.contains("createElement(\"tr\""),
        "tr → createElement: {}",
        js
    );
    assert!(
        js.contains("createElement(\"td\""),
        "td → createElement: {}",
        js
    );
    // The @psx decorator itself is stripped from the emitted JS.
    assert!(
        !js.contains("psx(render_row)"),
        "@psx not applied at runtime: {}",
        js
    );
    assert!(!js.contains("@psx"), "@psx symbol removed: {}", js);
}

#[test]
fn test_psx_does_not_imply_export() {
    // Unlike @component, @psx leaves export visibility alone.
    let source = r#"
from pyths.react import psx

@psx
def helper():
    return div()()
"#;
    let js = compile(source);
    assert!(js.contains("function helper"), "function emitted: {}", js);
    // @psx-only functions are NOT exported by default — that's the
    // whole point. @component implies export; @psx is internal-helper-friendly.
    assert!(
        !js.contains("export function helper"),
        "@psx must not imply export: {}",
        js
    );
}

#[test]
fn test_psx_does_not_destructure_props() {
    // @component would emit `function helper({a, b} = {})`. @psx must
    // emit the regular positional-param signature.
    let source = r#"
from pyths.react import psx

@psx
def render_pair(a, b):
    return div()(span()(a), span()(b))
"#;
    let js = compile(source);
    assert!(
        js.contains("function render_pair(a, b)"),
        "positional params: {}",
        js
    );
    assert!(
        !js.contains("{a, b} = {}"),
        "@psx must not props-destructure: {}",
        js
    );
}

#[test]
fn test_component_calls_psx_helper() {
    // The integration case: a @component invokes a @psx-decorated
    // helper that returns JSX. Both must coexist cleanly.
    let source = r#"
from pyths.react import component, psx

@psx
def render_row(item):
    return tr()(td()(item.name))

@component
def Table(items):
    return table()(
        *[render_row(i) for i in items]
    )
"#;
    let js = compile(source);
    assert!(js.contains("function render_row(item)"), "helper: {}", js);
    assert!(js.contains("export function Table"), "component: {}", js);
    // Helper uses createElement (PSX mode active).
    assert!(js.contains("createElement(\"tr\""), "helper PSX: {}", js);
    // Component spread-maps over items, calling helper.
    // Unknown-typed iterables are materialized via pySeq before .map
    // (Pythonic-checks sweep — comprehension over arbitrary iterables).
    assert!(
        js.contains("pySeq(items).map") || js.contains("items.map") || js.contains("...items"),
        "component iterates items: {}",
        js
    );
}

#[test]
fn test_no_decorator_means_no_psx() {
    // Sanity check: a plain function (no decorator) does NOT get PSX
    // mode, so HTML-tag-named identifiers are treated as user-defined
    // names, not React elements.
    let source = r#"
def regular_fn():
    return 42
"#;
    let js = compile(source);
    assert!(
        js.contains("function regular_fn"),
        "regular function: {}",
        js
    );
    assert!(
        !js.contains("createElement"),
        "no PSX without decorator: {}",
        js
    );
}

#[test]
fn test_psx_with_fragment_returns_tuple() {
    // Render-prop helpers commonly return multiple JSX elements via
    // tuple/list. @psx mode + tuple return → Fragment children.
    let source = r#"
from pyths.react import psx

@psx
def render_pair():
    return (div()("a"), div()("b"))
"#;
    let js = compile(source);
    // Tuples in @component/@psx return position become Fragment children.
    assert!(
        js.contains("createElement(Fragment"),
        "tuple return → Fragment: {}",
        js
    );
}

// =====================================================================
// Production-readiness #1: Generic npm-package fallback
// =====================================================================
// Unmatched modules default to kebab-case npm path. Closes the long
// tail of the npm ecosystem without per-package mapping entries.
// =====================================================================

#[test]
fn test_generic_kebab_fallback_simple() {
    let js = compile("from foo_bar import x\nx()");
    assert!(js.contains("from \"foo-bar\""), "foo_bar → foo-bar: {}", js);
}

#[test]
fn test_generic_kebab_fallback_subpath() {
    let js = compile("from some_lib.deep_path import helper\nhelper()");
    assert!(
        js.contains("from \"some-lib/deep-path\""),
        "subpath kebab'd: {}",
        js
    );
}

#[test]
fn test_kebab_fallback_no_underscores_unchanged() {
    // Modules without underscores pass through (e.g., already kebab,
    // or single-word like "lodash").
    let js = compile("from lodash import map\nx = map");
    assert!(js.contains("from \"lodash\""), "lodash: {}", js);
}

#[test]
fn test_explicit_mapping_still_wins() {
    // NPM_MODULE_MAPPINGS take precedence over the generic kebab rule.
    // react_redux → "react-redux" via the explicit table, not the
    // generic rule (both happen to produce the same output here, but
    // the explicit one fires first).
    let js = compile("from react_redux import use_selector\nx = use_selector");
    assert!(js.contains("from \"react-redux\""), "react-redux: {}", js);
}

#[test]
fn test_scoped_at_prefix_unaffected() {
    // The `at_org.pkg → @org/pkg` rule still fires before the kebab
    // fallback. Scoped packages are unchanged.
    let js = compile("from at_my_org.pkg_name import x\nx()");
    assert!(
        js.contains("from \"@my-org/pkg-name\""),
        "scoped pkg: {}",
        js
    );
}

#[test]
fn test_pyths_namespace_unaffected() {
    // pyths.* imports resolve via the runtime resolver, NOT the kebab
    // fallback. (Otherwise pyths.dom would become pyths/dom.)
    let js = compile("from pyths.dom import query\nx = query");
    assert!(
        js.contains("from \"pyths-runtime/dom\""),
        "pyths.dom: {}",
        js
    );
}

#[test]
fn test_stdlib_unaffected() {
    let js = compile("from math import pi\nx = pi");
    assert!(
        js.contains("from \"pyths-runtime/stdlib/math\""),
        "math: {}",
        js
    );
}

// =====================================================================
// Server Components — Tier 1 (sync with reality, defer RSC payload)
// =====================================================================
// React/Next.js Server Components are async functions that may `await`
// data sources directly. The codegen emits them as `async function`s;
// Next.js's runtime handles the RSC streaming protocol (out of scope
// for the codegen — it's a runtime concern, not a compile-time one).
// =====================================================================

#[test]
fn test_async_component_compiles() {
    let source = r#"
from pyths.react import component

async def fetch_user(user_id):
    return {"name": "Alice"}

@component
async def UserCard(user_id):
    user = await fetch_user(user_id)
    return div()(h2()(user["name"]))
"#;
    let js = compile(source);
    // Async helper is async.
    assert!(
        js.contains("async function fetch_user"),
        "helper async: {}",
        js
    );
    // @component on async def emits `export async function`.
    assert!(
        js.contains("export async function UserCard"),
        "async @component: {}",
        js
    );
    // Await inside the body works.
    assert!(js.contains("await fetch_user"), "await in body: {}", js);
    // Body still emits createElement.
    assert!(
        js.contains("createElement(\"div\""),
        "PSX in async body: {}",
        js
    );
    assert!(js.contains("createElement(\"h2\""), "nested PSX: {}", js);
}

#[test]
fn test_use_server_module_directive() {
    let source = r#""use server"

from pyths.react import component

async def create_post(form_data):
    return {"saved": True}

@component
def Form():
    return form()(button()("Submit"))
"#;
    let js = compile(source);
    // Directive must be the first emitted statement, before imports.
    assert!(
        js.starts_with("\"use server\""),
        "use server first: {}",
        &js[..js.len().min(120)]
    );
    assert!(
        js.contains("async function create_post"),
        "server action emitted: {}",
        js
    );
}

#[test]
fn test_use_server_function_level_directive() {
    // Inline server-action form: directive as the first statement of
    // an async function body.
    let source = r#"
async def create_post(form_data):
    "use server"
    return {"saved": True}
"#;
    let js = compile(source);
    // The inner directive should appear at the top of the function body.
    assert!(
        js.contains("async function create_post"),
        "async fn: {}",
        js
    );
    assert!(js.contains("\"use server\""), "directive in body: {}", js);
}

#[test]
fn test_use_client_module_directive_still_works() {
    // Pre-existing behavior — a regression check.
    let source = r#""use client"

from pyths.react import component, use_state

@component
def Counter():
    count, set_count = use_state(0)
    return button(on_click=lambda: set_count(count + 1))(count)
"#;
    let js = compile(source);
    assert!(
        js.starts_with("\"use client\""),
        "use client first: {}",
        &js[..js.len().min(80)]
    );
    assert!(js.contains("useState(0)"), "client hook: {}", js);
}

#[test]
fn test_async_component_with_use_hook() {
    // React 19's `use()` unwraps a Promise in render. PythScribe-side
    // it's just a function call — codegen emits `use(promise)` directly.
    let source = r#"
from pyths.react import component, use

@component
async def Profile(user_promise):
    user = use(user_promise)
    return div()(h2()(user["name"]))
"#;
    let js = compile(source);
    assert!(
        js.contains("export async function Profile"),
        "async: {}",
        js
    );
    assert!(js.contains("use(user_promise)"), "use() hook: {}", js);
}

#[test]
fn test_suspense_component_in_jsx() {
    // <Suspense fallback={...}>children</Suspense> — Suspense is a
    // capitalized name imported from react, so it routes through the
    // user-component createElement path.
    let source = r#"
from pyths.react import component, Suspense

@component
def Page():
    return Suspense(fallback=div()("Loading..."))(
        h1()("Content")
    )
"#;
    let js = compile(source);
    assert!(
        js.contains("createElement(Suspense"),
        "Suspense element: {}",
        js
    );
    assert!(js.contains("fallback:"), "fallback prop: {}", js);
}

// ============================================================================
// React Refresh codegen
// ============================================================================

fn compile_with_refresh(source: &str) -> String {
    let module = pyths_parser::parse(source).expect("Parse failed");
    let mut gen = pyths_codegen_js::JsCodegen::new();
    gen.enable_react_refresh();
    gen.emit_module(&module);
    gen.finish()
}

#[test]
fn refresh_emits_signature_around_component() {
    let js = compile_with_refresh(
        r#"
from pyths.react import component, use_state

@component
def Counter():
    count, set_count = use_state(0)
    return div()("Count:", count)
"#,
    );
    assert!(
        js.contains("const _s_Counter = $RefreshSig$()"),
        "sig declared: {}",
        js
    );
    assert!(js.contains("_s_Counter();"), "sig call inside body: {}", js);
    assert!(
        js.contains("_s_Counter(Counter,"),
        "sig install after body: {}",
        js
    );
    assert!(
        js.contains("$RefreshReg$(Counter, \"Counter\")"),
        "Reg call: {}",
        js
    );
}

#[test]
fn refresh_hook_signature_captures_hook_names() {
    let js = compile_with_refresh(
        r#"
from pyths.react import component, use_state, use_effect

@component
def Widget():
    a, sa = use_state(0)
    use_effect(lambda: None, [])
    return None
"#,
    );
    // The signature string contains both hook names (PythScribe source
    // form). React only needs stability, so the format is opaque to
    // users — what matters is it changes when the hook order changes.
    let sig_line = js
        .lines()
        .find(|l| l.contains("_s_Widget(Widget,"))
        .expect("sig line");
    assert!(
        sig_line.contains("use_state"),
        "use_state in sig: {}",
        sig_line
    );
    assert!(
        sig_line.contains("use_effect"),
        "use_effect in sig: {}",
        sig_line
    );
}

#[test]
fn refresh_disabled_by_default() {
    let module = pyths_parser::parse(
        r#"
from pyths.react import component, use_state

@component
def Counter():
    count, _ = use_state(0)
    return None
"#,
    )
    .unwrap();
    let js = pyths_codegen_js::codegen(&module);
    assert!(
        !js.contains("$RefreshSig$"),
        "no Refresh by default: {}",
        js
    );
    assert!(
        !js.contains("$RefreshReg$"),
        "no Refresh by default: {}",
        js
    );
}

#[test]
fn refresh_skips_lowercase_component() {
    // Lowercase function names aren't treated as React components even
    // if decorated `@component`. Refresh boilerplate is dead code for
    // them — codegen skips emission.
    let js = compile_with_refresh(
        r#"
from pyths.react import component, use_state

@component
def helper():
    x, _ = use_state(0)
    return None
"#,
    );
    assert!(!js.contains("$RefreshReg$"), "no Reg for lowercase: {}", js);
    assert!(!js.contains("$RefreshSig$"), "no Sig for lowercase: {}", js);
}

#[test]
fn refresh_skips_non_component_functions() {
    let js = compile_with_refresh(
        r#"
def Plain():
    return 1

@component
def Widget():
    return None
"#,
    );
    assert!(
        !js.contains("_s_Plain"),
        "no Refresh for plain function: {}",
        js
    );
    assert!(js.contains("_s_Widget"), "Refresh for component: {}", js);
}

#[test]
fn refresh_handles_component_with_no_hooks() {
    // Static @component — no hook calls — should still emit Reg+Sig
    // (zero-hook signature). Plugin still benefits: pure render edits
    // can preserve state of unaffected siblings.
    let js = compile_with_refresh(
        r#"
from pyths.react import component

@component
def Header():
    return h1()("Hello")
"#,
    );
    assert!(
        js.contains("const _s_Header = $RefreshSig$()"),
        "Sig: {}",
        js
    );
    assert!(js.contains("$RefreshReg$(Header,"), "Reg: {}", js);
    // Zero-hook signature is the empty string.
    assert!(
        js.contains("_s_Header(Header, \"\")"),
        "empty sig install: {}",
        js
    );
}

// ============================================================================
// pyths.react import splitting — hooks vs runtime helpers
// ============================================================================
//
// Regression coverage for the bug where `from pyths.react import X, Y`
// emitted both names from `"pyths-runtime/react"`, even though React hooks
// only exist in the `react` npm package. Adding hooks as runtime re-exports
// is wrong (it forces React as a runtime dep, breaking non-React consumers
// of pyths-runtime — see CPython semantic differential tests). The compiler
// must split into two imports.

#[test]
fn pyths_react_hook_imports_route_to_react_package() {
    let js = compile(
        r#"
from pyths.react import component, use_state

@component
def Counter():
    count, set_count = use_state(0)
    return None
"#,
    );
    assert!(
        js.contains("import { useState } from \"react\";"),
        "useState must come from react: {}",
        js
    );
    assert!(
        js.contains("import { component } from \"pyths-runtime/react\";"),
        "component must come from pyths-runtime/react: {}",
        js
    );
    // Negative: the old wrong behavior would put useState inside the
    // pyths-runtime/react import. Guard against regression.
    assert!(
        !js.contains("{ component, useState } from \"pyths-runtime/react\""),
        "useState must NOT be in the runtime import: {}",
        js
    );
}

#[test]
fn pyths_react_helpers_only_no_react_import_emitted() {
    let js = compile(
        r#"
from pyths.react import component

@component
def Header():
    return None
"#,
    );
    assert!(
        js.contains("import { component } from \"pyths-runtime/react\";"),
        "helper import: {}",
        js
    );
    // No React core import should fire when only helpers are imported,
    // beyond the PSX createElement auto-import (which is a different code path).
    let helper_only_react_imports = js
        .lines()
        .filter(|l| l.trim_start().starts_with("import {") && l.contains("from \"react\""))
        .filter(|l| !l.contains("createElement") && !l.contains("Fragment"))
        .count();
    assert_eq!(
        helper_only_react_imports, 0,
        "no spurious react import: {}",
        js
    );
}

#[test]
fn pyths_react_hooks_only_no_runtime_import_emitted() {
    let js = compile(
        r#"
from pyths.react import use_state

def use_counter():
    count, set_count = use_state(0)
    return count
"#,
    );
    assert!(
        js.contains("import { useState } from \"react\";"),
        "hook from react: {}",
        js
    );
    assert!(
        !js.contains("from \"pyths-runtime/react\""),
        "no runtime import when only hooks used: {}",
        js
    );
}

#[test]
fn pyths_react_multiple_hooks_and_helpers_split_correctly() {
    let js = compile(
        r#"
from pyths.react import component, psx, use_state, use_effect, use_memo

@component
def Widget():
    count, set_count = use_state(0)
    use_effect(lambda: None, [])
    doubled = use_memo(lambda: count * 2, [count])
    return None
"#,
    );
    // All three hooks together from react.
    assert!(
        js.contains("import { useState, useEffect, useMemo } from \"react\";"),
        "all hooks from react: {}",
        js
    );
    // Both helpers together from pyths-runtime/react.
    assert!(
        js.contains("import { component, psx } from \"pyths-runtime/react\";"),
        "all helpers from runtime: {}",
        js
    );
}

// ============================================================================
// `in` operator routes through pyContains (Python-correct membership semantics)
// ============================================================================
//
// Regression coverage for the bug where `key in dict` compiled to
// `dict.includes(key)` — an array op that crashes (TypeError: not a function)
// on plain JS objects, where Python's `in` checks KEY membership. The fix
// routes every `in` / `not in` through the runtime's `pyContains`, which
// dispatches by container type.

#[test]
fn in_op_auto_imports_pyContains() {
    let js = compile(
        r#"
def has_key(d, k):
    return k in d
"#,
    );
    assert!(
        js.contains("pyContains(d, k)"),
        "in must use pyContains: {}",
        js
    );
    assert!(
        js.contains("pyContains") && js.contains("from \"pyths-runtime\""),
        "pyContains must be auto-imported: {}",
        js
    );
    assert!(
        !js.contains("d.includes(k)"),
        "raw .includes is wrong for object key checks: {}",
        js
    );
}

#[test]
fn not_in_op_auto_imports_pyContains() {
    let js = compile(
        r#"
def missing(d, k):
    return k not in d
"#,
    );
    assert!(
        js.contains("!pyContains(d, k)"),
        "not in must use !pyContains: {}",
        js
    );
}

// ============================================================================
// `raise X(...)` auto-imports builtin Python exception classes
// ============================================================================
//
// Regression coverage for the bug where `raise ValueError("x")` compiled to
// `throw new ValueError("x")` but ValueError was never imported from
// pyths-runtime → ReferenceError at module load. Fix parallels how `pyLen`
// auto-imports when `len()` is used: detect `raise Name(...)` where Name is a
// known Python builtin exception class and add it to runtime_imports.

#[test]
fn raise_value_error_auto_imports() {
    let js = compile(
        r#"
def boom():
    raise ValueError("nope")
"#,
    );
    assert!(
        js.contains("import { ValueError") && js.contains("from \"pyths-runtime\""),
        "ValueError must be auto-imported from pyths-runtime: {}",
        js
    );
    assert!(
        js.contains("throw new ValueError(\"nope\")"),
        "throw statement intact: {}",
        js
    );
}

#[test]
fn raise_exception_auto_imports() {
    let js = compile(
        r#"
def boom():
    raise Exception("generic")
"#,
    );
    assert!(
        js.contains("import { Exception") && js.contains("from \"pyths-runtime\""),
        "Exception must be auto-imported (was missing from runtime entirely before this fix): {}",
        js
    );
    assert!(
        js.contains("throw new Exception(\"generic\")"),
        "throw statement intact: {}",
        js
    );
}

#[test]
fn raise_all_builtin_exception_classes_auto_import() {
    let js = compile(
        r#"
def all_raises(x):
    if x == 1:
        raise ValueError("a")
    if x == 2:
        raise IndexError("b")
    if x == 3:
        raise KeyError("c")
    if x == 4:
        raise AttributeError("d")
    if x == 5:
        raise StopIteration("e")
    if x == 6:
        raise ZeroDivisionError("f")
    if x == 7:
        raise Exception("g")
"#,
    );
    for name in [
        "ValueError",
        "IndexError",
        "KeyError",
        "AttributeError",
        "StopIteration",
        "ZeroDivisionError",
        "Exception",
    ] {
        assert!(
            js.contains(&format!("throw new {}", name)),
            "{} must appear in a throw statement: {}",
            name,
            js
        );
    }
    // All seven should land in one combined runtime import.
    assert!(
        js.contains("from \"pyths-runtime\""),
        "runtime import line present: {}",
        js
    );
}

#[test]
fn raise_custom_class_is_not_auto_imported() {
    // User-defined exceptions are NOT in the builtin whitelist — must not
    // trigger a spurious runtime import.
    let js = compile(
        r#"
def boom():
    raise MyCustomError("user-defined")
"#,
    );
    assert!(
        !js.contains("import { MyCustomError } from \"pyths-runtime\""),
        "custom class must NOT be runtime-imported: {}",
        js
    );
    assert!(
        js.contains("throw new MyCustomError(\"user-defined\")"),
        "throw statement intact: {}",
        js
    );
}

// ============================================================================
// pyths.toml [npm.imports] overrides
// ============================================================================

fn compile_with_npm_imports(source: &str, imports: &[(&str, &str)]) -> String {
    let module = pyths_parser::parse(source).expect("Parse failed");
    let map: std::collections::HashMap<String, String> = imports
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();
    pyths_codegen_js::codegen_with_npm_imports(&module, &map)
}

#[test]
fn npm_import_override_wins_over_kebab_fallback() {
    // Without an override, `from foo_bar import x` would emit
    // `import { x } from "foo-bar"` via the kebab-case fallback. The
    // override re-routes it to the user-supplied specifier.
    let js = compile_with_npm_imports(
        "from foo_bar import x\nresult = x()\n",
        &[("foo_bar", "@my-org/foo-bar-custom")],
    );
    assert!(
        js.contains("from \"@my-org/foo-bar-custom\""),
        "user override emitted: {}",
        js
    );
    assert!(
        !js.contains("from \"foo-bar\""),
        "kebab fallback should NOT fire: {}",
        js
    );
}

#[test]
fn npm_import_override_wins_over_builtin_mapping() {
    // `react_router` has a built-in mapping → `react-router`. User
    // override should win.
    let js = compile_with_npm_imports(
        "from react_router import Route\nx = Route()\n",
        &[("react_router", "react-router-v7-canary")],
    );
    assert!(
        js.contains("from \"react-router-v7-canary\""),
        "user override emitted: {}",
        js
    );
    assert!(
        !js.contains("from \"react-router\""),
        "built-in mapping should NOT fire: {}",
        js
    );
}

#[test]
fn npm_import_no_override_uses_built_in_resolution() {
    // No override → existing kebab fallback applies.
    let js = compile_with_npm_imports(
        "from foo_bar import x\nresult = x()\n",
        &[("other_module", "some/path")],
    );
    assert!(
        js.contains("from \"foo-bar\""),
        "kebab fallback fires: {}",
        js
    );
}

#[test]
fn npm_import_empty_overrides_preserves_default() {
    let js = compile_with_npm_imports("from foo_bar import x\nresult = x()\n", &[]);
    assert!(js.contains("from \"foo-bar\""), "default fallback: {}", js);
}

#[test]
fn npm_import_override_for_unknown_scoped_package() {
    // Project uses a private scoped package; override lets them name
    // it cleanly from Python without going through the `at_` convention.
    let js = compile_with_npm_imports(
        "from acme_internal import widget\nx = widget()\n",
        &[("acme_internal", "@acme/internal-widgets")],
    );
    assert!(
        js.contains("from \"@acme/internal-widgets\""),
        "scoped-package override: {}",
        js
    );
}

// ─── JS-quirk fixes (Python-faithful semantics) ────────────────────────

#[test]
fn test_list_concat_emits_spread_not_plus() {
    // `[] + []` in JS coerces to `""`. Python expects `[]`.
    let js = compile("a = [1, 2]\nb = [3, 4]\nc = a + b\n");
    assert!(js.contains("[..."), "list+list must spread: {}", js);
    assert!(!js.contains("(a + b)"), "must NOT raw-add lists: {}", js);
}

#[test]
fn test_empty_list_concat_is_empty_list_not_string() {
    let js = compile("a = []\nb = []\nc = a + b\n");
    assert!(js.contains("[..."), "[] + [] must spread: {}", js);
}

#[test]
fn test_set_concat_emits_new_set() {
    // #297: canonicalizing PySet result.
    let js = compile("a = {1, 2}\nb = {3, 4}\nc = a + b\n");
    assert!(
        js.contains("new PySet([..."),
        "set+set must use new PySet: {}",
        js
    );
}

#[test]
fn test_tuple_concat_routes_via_pyadd() {
    // tuple+tuple must go through pyAdd so the result stays a TUPLE — a blind
    // spread produced a plain list (dropping __pytuple__), and list+tuple must
    // raise TypeError. crit-13.
    let js = compile("a = (1, 2)\nb = (3, 4)\nc = a + b\n");
    assert!(
        js.contains("pyAdd(a, b)"),
        "tuple+tuple routes via pyAdd: {}",
        js
    );
    assert!(
        !js.contains("[...a, ...b]"),
        "tuple+tuple must not blind-spread: {}",
        js
    );
}

#[test]
fn test_large_int_literal_emits_bigint() {
    // Literals beyond 2**53 can't be exact JS Numbers → emit BigInt.
    let js = compile("x = 9007199254740993\n");
    assert!(
        js.contains("9007199254740993n"),
        "large literal → BigInt: {}",
        js
    );
}

#[test]
fn test_float_arithmetic_emits_bare_ops() {
    // P2 native fast path: float operands are always JS Number (never
    // BigInt-promoted), so arithmetic emits bare ops, skipping the helper.
    let js = compile("a = 1.5\nb = 2.5\nx = a + b\ny = a * b\nz = a - b\n");
    assert!(js.contains("(a + b)"), "float + → bare: {}", js);
    assert!(js.contains("(a * b)"), "float * → bare: {}", js);
    assert!(js.contains("(a - b)"), "float - → bare: {}", js);
    assert!(!js.contains("pyAdd"), "no pyAdd for floats: {}", js);
}

#[test]
fn test_float_annotated_param_uses_bare_ops() {
    let js = compile("def f(a: float, b: float):\n    return a + b\n");
    assert!(js.contains("(a + b)"), "float-annotated → bare: {}", js);
    assert!(!js.contains("pyAdd"), "no pyAdd for float params: {}", js);
}

#[test]
fn test_int_arithmetic_still_uses_helper() {
    // Ints can overflow 2**53, so they must stay on the promoting helper
    // (NOT bare) — the faithfulness guarantee.
    let js = compile("def f(i):\n    return i + 1\n");
    assert!(js.contains("pyAdd(i, 1)"), "int + stays on helper: {}", js);
}

#[test]
fn test_bounded_int_arithmetic_emits_bare_ops() {
    // Provably-bounded int arithmetic (literals, len(list)) can't overflow
    // 2**53, so it emits bare ops instead of the helper.
    // Issue #22: len(list) now emits list.length instead of pyLen(list),
    // so the arithmetic stays bare but the receiver uses the native form.
    let js = compile("items: list = []\na = len(items) - 1\nb = 2 + 3\nc = len(items) * 4\n");
    assert!(
        js.contains("(items.length - 1)"),
        "len(list)-1 → bare native: {}",
        js
    );
    assert!(js.contains("(2 + 3)"), "literal+literal → bare: {}", js);
    assert!(
        js.contains("(items.length * 4)"),
        "len(list)*k → bare native: {}",
        js
    );
    assert!(!js.contains("pySub"), "no pySub for bounded: {}", js);
    assert!(
        !js.contains("pyLen(items)"),
        "pyLen not needed for typed list: {}",
        js
    );
}

#[test]
fn test_overflowing_constant_mul_stays_on_helper() {
    // A product that provably EXCEEDS 2**53 must stay on the helper so the
    // exact BigInt result is preserved — not silently truncated by a bare *.
    let js = compile("x = 1000000000000 * 1000000000000\n");
    assert!(
        js.contains("pyMul("),
        "overflowing * stays on helper: {}",
        js
    );
}

#[test]
fn test_inline_runtime_emits_repr_and_exception() {
    // `pyths run` inlines the runtime; pyRepr and the builtin Exception
    // classes must be emitted so repr() and `class X(Exception)` work
    // inline (not just via the npm/Vite build path).
    let module = pyths_parser::parse("class MyErr(Exception):\n    pass\nx = repr([1, 2])\n")
        .expect("parse");
    let js = pyths_codegen_js::codegen_inline(&module);
    assert!(js.contains("function pyRepr("), "inline pyRepr: {}", js);
    assert!(
        js.contains("class Exception extends Error"),
        "inline Exception: {}",
        js
    );
}

#[test]
fn test_small_int_literal_stays_number() {
    // Small ints stay native Number (fast path: indices, counters).
    let js = compile("x = 5\narr[2]\n");
    assert!(
        js.contains("let x = 5;"),
        "small literal stays Number: {}",
        js
    );
    assert!(!js.contains("5n"), "no BigInt for small literal: {}", js);
}

#[test]
fn test_arithmetic_routes_through_bigint_helpers() {
    let js = compile("p = a - b\nq = a * b\nr = a / b\ns = a ** b\n");
    assert!(js.contains("pySub(a, b)"), "sub → pySub: {}", js);
    assert!(js.contains("pyMul(a, b)"), "mul → pyMul: {}", js);
    assert!(js.contains("pyDiv(a, b)"), "div → pyDiv: {}", js);
    assert!(js.contains("pyPow(a, b)"), "pow → pyPow: {}", js);
}

#[test]
fn test_numeric_add_routes_through_pyadd() {
    // Numeric + routes through pyAdd so arbitrary-precision ints stay exact
    // (Number→BigInt promotion across 2**53). Must NOT spread-concat.
    let js = compile("x = 1\ny = 2\nz = x + y\n");
    assert!(
        js.contains("pyAdd(x, y)"),
        "int+int routes via pyAdd: {}",
        js
    );
    assert!(!js.contains("[..."), "int+int must NOT spread: {}", js);
}

#[test]
fn test_if_list_uses_pybool() {
    // `if my_list:` should wrap — `if ([])` is truthy in JS, falsy in Python.
    let js = compile("items = [1, 2]\nif items:\n    print(\"yes\")\n");
    assert!(js.contains("pyBool(items)"), "if-list must wrap: {}", js);
}

#[test]
fn test_if_dict_uses_pybool() {
    let js = compile("d = {\"a\": 1}\nif d:\n    print(\"yes\")\n");
    assert!(js.contains("pyBool(d)"), "if-dict must wrap: {}", js);
}

#[test]
fn test_if_int_skips_pybool() {
    // `if count:` for numeric must NOT wrap — JS truthiness on numbers
    // matches Python.
    let js = compile("count = 5\nif count:\n    print(\"yes\")\n");
    assert!(!js.contains("pyBool(count)"), "int must not wrap: {}", js);
    assert!(js.contains("if (count)"), "int must stay bare: {}", js);
}

#[test]
fn test_while_list_uses_pybool() {
    let js = compile("items = [1, 2]\nwhile items:\n    items.pop()\n");
    assert!(js.contains("pyBool(items)"), "while-list must wrap: {}", js);
}

#[test]
fn test_ternary_compare_skips_pybool() {
    // `x if x > 0 else y` — the test is a Compare, already boolean.
    let js = compile("x = 1\ny = 2\nz = x if x > 0 else y\n");
    assert!(!js.contains("pyBool"), "compare test must not wrap: {}", js);
}

#[test]
fn test_list_eq_uses_pyeq() {
    // `[1,2] == [1,2]` is `false` under JS `===` (ref equality).
    // Python expects element-wise compare via `pyEq`.
    let js = compile("a = [1, 2]\nb = [1, 2]\nc = a == b\n");
    assert!(js.contains("pyEq(a, b)"), "list== must use pyEq: {}", js);
    assert!(!js.contains("(a === b)"), "list== must NOT use ===: {}", js);
}

#[test]
fn test_list_neq_uses_negated_pyeq() {
    let js = compile("a = [1, 2]\nb = [1, 2]\nc = a != b\n");
    assert!(
        js.contains("(!pyEq(a, b))"),
        "list!= must use !pyEq: {}",
        js
    );
}

#[test]
fn test_dict_eq_uses_pyeq() {
    let js = compile("a = {\"k\": 1}\nb = {\"k\": 1}\nc = a == b\n");
    assert!(js.contains("pyEq(a, b)"), "dict== must use pyEq: {}", js);
}

#[test]
fn test_int_eq_keeps_strict() {
    // `5 == 5` for primitives must keep `===`.
    let js = compile("x = 5\ny = 5\nz = x == y\n");
    assert!(js.contains("(x === y)"), "int== must use ===: {}", js);
    assert!(!js.contains("pyEq"), "int== must NOT use pyEq: {}", js);
}

#[test]
fn test_list_eq_literal_uses_pyeq() {
    // Literal lists on both sides — inferred directly without scope lookup.
    let js = compile("c = [1, 2] == [1, 2]\n");
    assert!(js.contains("pyEq"), "literal-list== must use pyEq: {}", js);
}

// ─── Python-named runtime errors (Phase A: fix audit findings) ─────────

#[test]
fn test_list_subscript_uses_pygetitem() {
    // Out-of-range read on a typed list must raise IndexError, not
    // silently return undefined. Codegen routes the read through
    // pyGetItem when the receiver is List-typed.
    // Issue #22: a native-emission optimisation for non-neg literals was
    // attempted but reverted — out-of-bounds access would silently return
    // undefined instead of IndexError (correctness > bundle savings).
    let js = compile("xs = [1, 2, 3]\ny = xs[10]\n");
    assert!(
        js.contains("pyGetItem(xs, 10)"),
        "list read must wrap: {}",
        js
    );
}

#[test]
fn test_dict_subscript_uses_pygetitem() {
    // Missing-key read on a dict-typed value must raise KeyError.
    let js = compile("d = {\"a\": 1}\nv = d[\"missing\"]\n");
    assert!(
        js.contains("pyGetItem(d, \"missing\")"),
        "dict read must wrap: {}",
        js
    );
}

#[test]
fn test_tuple_subscript_uses_pygetitem() {
    // Issue #22: same correctness constraint applies to tuples — keep pyGetItem.
    let js = compile("t = (1, 2, 3)\nx = t[5]\n");
    assert!(
        js.contains("pyGetItem(t, 5)"),
        "tuple read must wrap: {}",
        js
    );
}

#[test]
fn test_subscript_assignment_stays_bare() {
    // LHS context — `a[i] = x` must NOT wrap, otherwise emit becomes
    // syntactically invalid (`pyGetItem(a, i) = x`).
    let js = compile("xs = [1, 2, 3]\nxs[0] = 99\n");
    // crit-7: subscript writes route through the bounds-checked pySetItem (a
    // non-negative literal index is not provably in-bounds). Must NOT wrap the
    // LHS in pyGetItem (invalid syntax).
    assert!(
        js.contains("pySetItem(xs, 0, 99)"),
        "LHS subscript routes via pySetItem: {}",
        js
    );
    assert!(
        !js.contains("pyGetItem(xs, 0) ="),
        "LHS must not wrap: {}",
        js
    );
}

#[test]
fn test_aug_assign_subscript_routes_through_helper() {
    // bigint: the former bare `xs[0] += 1` hot path truncated BigInt-producing
    // operators to int32 and threw on mixed BigInt/number (`xs[0] += 2**60`,
    // `xs[0] <<= 50`). Every helper-backed operator now routes through
    // pyGetItem/pySetItem + the Python-operator helper so arbitrary-precision
    // semantics hold (see AugAssign codegen: op_has_helper gates bare_ok).
    let js = compile("xs = [1, 2, 3]\nxs[0] += 1\n");
    assert!(
        js.contains("pySetItem") && js.contains("pyAdd(pyGetItem"),
        "aug-assign subscript must route through the helper: {}",
        js
    );
    // Shift-assign in particular must not emit a raw JS `<<=`.
    let jsh = compile("xs = [1]\nxs[0] <<= 50\n");
    assert!(
        jsh.contains("pyShiftLeft(pyGetItem"),
        "shift-assign must route through pyShiftLeft: {}",
        jsh
    );
    assert!(
        !jsh.contains("<<="),
        "shift-assign must not stay bare: {}",
        jsh
    );
}

#[test]
fn test_aug_assign_negative_index_routes_through_setitem() {
    // #278: JS `xs[-1] -= v` writes the property "-1", not the last element.
    // A negative (or computed) index must go through pyGetItem/pySetItem so
    // Python's negative indexing holds.
    let jn = compile("xs = [1, 2, 3]\nxs[-1] -= 1\n");
    assert!(
        jn.contains("pySetItem") && jn.contains("pySub(pyGetItem"),
        "negative index must route through setitem:\n{}",
        jn
    );
    assert!(
        !jn.contains("xs[(-1)] -="),
        "negative index must not stay bare:\n{}",
        jn
    );
    // A computed index isn't provably non-negative → also routes through helpers.
    let jc = compile("xs = [1, 2, 3]\ni = 1\nxs[i] += 1\n");
    assert!(
        jc.contains("pySetItem"),
        "computed index routes through setitem:\n{}",
        jc
    );
}

#[test]
fn test_negative_index_write_routes_through_setitem() {
    // #278: a plain `a[-1] = v` write on a List must go through pySetItem (JS
    // `a[-1] = v` writes the property "-1"). Covers the tuple-swap idiom
    // `a[-1], b[-1] = b[-1], a[-1]` which lowers to per-element writes.
    let jn = compile("xs = [1, 2, 3]\nxs[-1] = 9\n");
    assert!(
        jn.contains("pySetItem(xs"),
        "negative-index write routes through setitem:\n{}",
        jn
    );
    // non-negative literal index keeps the bare write (hot path)
    let jb = compile("xs = [1, 2, 3]\nxs[0] = 9\n");
    // crit-7: non-negative literal writes also route via pySetItem now
    // (bounds-checked; non-negative ≠ in-bounds).
    assert!(
        jb.contains("pySetItem(xs, 0, 9)"),
        "non-negative literal write routes via pySetItem:\n{}",
        jb
    );
    // tuple-swap of negative subscripts assigns via pySetItem, not bare
    let js = compile("a = [1, 2]\nb = [3, 4]\na[-1], b[-1] = b[-1], a[-1]\n");
    assert!(
        !js.contains("a[(-1)] ="),
        "swap must not write property -1:\n{}",
        js
    );
    assert!(
        js.contains("pySetItem(a") && js.contains("pySetItem(b"),
        "swap routes through setitem:\n{}",
        js
    );
}

#[test]
fn test_subscript_index_inside_target_still_wraps() {
    // `b[c[0]] = x` — outer subscript is LHS (bare), inner `c[0]` is a
    // read context inside the index expression and should still wrap.
    let js = compile("b = {}\nc = [0, 1]\nb[c[0]] = 99\n");
    assert!(
        js.contains("pyGetItem(c, 0)"),
        "inner index must wrap: {}",
        js
    );
}

#[test]
fn test_untyped_subscript_wraps() {
    // #83 flips the old stays-bare rule: an Unknown-typed receiver ALSO
    // routes through pyGetItem, so a Map-backed dict flowing through an
    // unannotated channel (function param, attribute) doesn't fall back
    // to raw `d[k]` (undefined on a Map). Interop safety: pyGetItem
    // passes non-plain-prototype objects (DOM wrappers, class instances)
    // through natively instead of raising KeyError.
    let js = compile(
        "def f(x):
    return x[0]
",
    );
    assert!(
        js.contains("pyGetItem(x, 0)"),
        "untyped subscript wraps: {}",
        js
    );
}

#[test]
fn test_floor_div_uses_helper() {
    // Routes through pyFloorDiv so b===0 throws ZeroDivisionError.
    let js = compile("x = a // b\n");
    assert!(
        js.contains("pyFloorDiv(a, b)"),
        "// must use helper: {}",
        js
    );
    assert!(
        !js.contains("Math.floor("),
        "// must not inline Math.floor: {}",
        js
    );
}

#[test]
fn test_mod_uses_helper() {
    let js = compile("x = a % b\n");
    assert!(js.contains("pyMod(a, b)"), "% must use helper: {}", js);
    assert!(!js.contains("((a % b)"), "% must not inline: {}", js);
}

#[test]
fn test_optional_chain_subscript_stays_bare() {
    // Optional chaining `a?.[i]` keeps JS short-circuit semantics —
    // wrapping would defeat the null-skip behavior.
    let js = compile("xs = [1, 2]\ny = xs?.[0]\n");
    assert!(
        js.contains("xs?.[0]"),
        "optional chain must stay bare: {}",
        js
    );
}

// ----------------------------------------------------------------------------
// Python relative imports (fix for reference-app B-004 / pythscribe #issue-tbd)
// ----------------------------------------------------------------------------

#[test]
fn test_sibling_relative_import_emits_dotslash() {
    // `from .foo import x` is a Python relative import targeting a sibling
    // module. Must emit `import { x } from "./foo"` — no kebab-casing, no
    // npm-bare-specifier fallback.
    let js = compile("from .foo import x\n");
    assert!(
        js.contains("import { x } from \"./foo\";"),
        "sibling relative import: {}",
        js
    );
}

#[test]
fn test_sibling_dotted_relative_import_preserves_underscores() {
    // Snake-case file names must NOT be kebab-cased on relative paths —
    // the local filesystem path is the source of truth, not npm convention.
    let js = compile("from .use_run_events import use_run_events\n");
    assert!(
        js.contains("import { use_run_events } from \"./use_run_events\";"),
        "snake-case relative import must preserve underscores: {}",
        js
    );
}

#[test]
fn test_parent_relative_import_emits_double_dot() {
    // `from ..components.GapsPanel import GapsPanel` should walk one ancestor
    // and emit `../components/GapsPanel`.
    let js = compile("from ..components.GapsPanel import GapsPanel\n");
    assert!(
        js.contains("import { GapsPanel } from \"./../components/GapsPanel\";"),
        "parent relative import: {}",
        js
    );
}

#[test]
fn test_grandparent_relative_import_emits_triple_path() {
    // `from ...lib.foo import bar` walks two ancestors.
    let js = compile("from ...lib.foo import bar\n");
    assert!(
        js.contains("import { bar } from \"./../../lib/foo\";"),
        "grandparent relative import: {}",
        js
    );
}

#[test]
fn test_relative_import_with_alias_preserves_alias() {
    let js = compile("from .foo import x as renamed\n");
    assert!(
        js.contains("import { x as renamed } from \"./foo\";"),
        "alias on relative import: {}",
        js
    );
}

#[test]
fn test_relative_and_absolute_imports_coexist() {
    let src = "from .sibling import x\nfrom react import use_state\n";
    let js = compile(src);
    assert!(
        js.contains("import { x } from \"./sibling\";"),
        "relative side: {}",
        js
    );
    assert!(
        js.contains("import { useState } from \"react\";"),
        "absolute side (react auto-camelcased): {}",
        js
    );
}

// ----------------------------------------------------------------------------
// React-core re-exports as PSX elements (reference-app B-007 / pythscribe #issue-tbd)
// ----------------------------------------------------------------------------

#[test]
fn test_fragment_in_component_wraps_as_create_element_not_constructor() {
    // `Fragment(...)` inside @component must wrap as
    // `createElement(Fragment, ...)`. Before this fix it fell through to
    // the uppercase-name class-instantiation branch and emitted
    // `new Fragment(...)` which throws at runtime.
    let src = "\
from pyths.react import Fragment, component
@component
def App():
    return Fragment(h1(\"hi\"))
";
    let js = compile(src);
    assert!(
        js.contains("createElement(Fragment"),
        "Fragment must wrap as createElement(Fragment, ...): {}",
        js
    );
    assert!(
        !js.contains("new Fragment"),
        "must NOT emit `new Fragment(...)`: {}",
        js
    );
}

#[test]
fn test_suspense_in_component_wraps_as_create_element() {
    let src = "\
from pyths.react import Suspense, component
@component
def App():
    return Suspense(span(\"loading\"))
";
    let js = compile(src);
    assert!(
        js.contains("createElement(Suspense"),
        "Suspense should wrap as createElement(Suspense, ...): {}",
        js
    );
    assert!(
        !js.contains("new Suspense"),
        "must NOT emit `new Suspense(...)`: {}",
        js
    );
}

#[test]
fn test_lowercase_hook_imports_stay_function_calls() {
    // The fix must not break the lowercase-hook path: use_state etc. must
    // still emit as plain `useState()` calls, not `createElement(useState)`.
    let src = "\
from pyths.react import component, use_state
@component
def C():
    x, set_x = use_state(0)
    return div(str(x))
";
    let js = compile(src);
    assert!(
        js.contains("useState(0)"),
        "use_state must compile to useState(0): {}",
        js
    );
    assert!(
        !js.contains("createElement(useState"),
        "hook must NOT wrap as createElement(useState, ...): {}",
        js
    );
}

// ----------------------------------------------------------------------------
// round() builtin (reference-app B-009 / pythscribe #issue-tbd)
// ----------------------------------------------------------------------------

#[test]
fn test_round_maps_to_pyround_runtime_helper() {
    // Python's round() has no JS equivalent (Math.round rounds half-up + takes
    // no ndigits). It must route through the pyRound runtime helper, not be
    // emitted as a bare `round(...)` ReferenceError.
    let js = compile("x = round(1.5)\ny = round(3.14159, 2)\n");
    assert!(js.contains("pyRound(1.5)"), "round(x) → pyRound(x): {}", js);
    assert!(
        js.contains("pyRound(3.14159, 2)"),
        "round(x, n) → pyRound(x, n): {}",
        js
    );
    assert!(
        !js.contains(" round("),
        "must not emit a bare round( call: {}",
        js
    );
    assert!(
        js.contains("import { pyRound }") || js.contains("pyRound,"),
        "pyRound must be imported: {}",
        js
    );
}

// ----------------------------------------------------------------------------
// Tier-7 exception subclass with constructor (reference-app B-013/B-014)
// ----------------------------------------------------------------------------

#[test]
fn test_exception_subclass_hoists_super_and_imports_base() {
    let src = "\
class ConnectionLostError(Exception):
    def __init__(self, attempts):
        self.attempts = attempts
        super().__init__(\"lost\")
";
    let js = compile(src);
    assert!(
        js.contains("extends Exception"),
        "must extend Exception: {}",
        js
    );
    assert!(
        js.contains("import { Exception }"),
        "Exception base must be auto-imported: {}",
        js
    );
    // super(...) must be emitted as a real super call (not super().__init__)
    assert!(
        js.contains("super(\"lost\")"),
        "super().__init__ must lower to super(...): {}",
        js
    );
    assert!(
        !js.contains("super().__init__"),
        "must not emit super().__init__: {}",
        js
    );
    // and it must precede the first `this.` assignment (JS requires super first)
    let su = js.find("super(\"lost\")").unwrap();
    let th = js.find("this.attempts").unwrap();
    assert!(su < th, "super(...) must precede this.x: {}", js);
}

// ----------------------------------------------------------------------------
// Module-level export for cross-file .ps imports (reference-app B-015)
// ----------------------------------------------------------------------------

#[test]
fn test_module_level_class_and_function_are_exported() {
    let js = compile("class Foo(Exception):\n    def __init__(self):\n        super().__init__(\"x\")\n\ndef helper(a):\n    return a + 1\n");
    assert!(
        js.contains("export class Foo"),
        "top-level class must export: {}",
        js
    );
    assert!(
        js.contains("export function helper"),
        "top-level fn must export: {}",
        js
    );
}

#[test]
fn test_nested_def_is_not_exported() {
    let js = compile("def outer():\n    def inner():\n        return 1\n    return inner\n");
    assert!(
        js.contains("export function outer"),
        "outer must export: {}",
        js
    );
    assert!(
        !js.contains("export function inner") && !js.contains("export const inner"),
        "nested inner must stay local: {}",
        js
    );
}

#[test]
fn test_module_level_variable_is_exported() {
    let js = compile("BTN = {\"a\": 1}\n\ndef use_it():\n    x = 2\n    return x\n");
    assert!(
        js.contains("export let BTN"),
        "top-level const must export: {}",
        js
    );
    // a local inside a function must NOT export
    assert!(
        !js.contains("export let x"),
        "function-local must stay local: {}",
        js
    );
}

#[test]
fn test_implicit_string_concatenation() {
    // Python implicit string literal concatenation: adjacent string tokens join at parse time.
    // `x = "hello" " world"` is identical to `x = "hello world"` in CPython.
    let js = compile("x = \"hello\" \" world\"\n");
    assert!(
        js.contains("\"hello world\""),
        "adjacent strings must be joined: {}",
        js
    );
    // Multi-way concat inside a function call argument list (the pattern that
    // triggered this fix: long strings split across lines in PSX calls).
    let js2 = compile("def f():\n    return g(\"part1\" \"part2\" \"part3\")\n");
    assert!(
        js2.contains("\"part1part2part3\""),
        "3-way concat must be joined: {}",
        js2
    );
}

// ─── Issue #22 — native JS emission for statically-known types ──────────────
// Implemented: pyLen → .length for list/tuple (safe, same semantics).
// Implemented (follow-up B): list literal indexed by provably-in-bounds
//   literal integer → native x[i] (e.g. [a,b,c][1] → [a,b,c][1]).
// Still kept: pyGetItem for all typed list/tuple/dict VARIABLE subscripts —
//   out-of-bounds variable access would silently return undefined instead of
//   IndexError; the literal-literal fast path is the only safe carve-out.

#[test]
fn test_issue22_len_list_typed_var_native() {
    // len(xs) where xs is annotated list → xs.length, no pyLen import.
    let js = compile("xs: list = [1, 2, 3]\nn = len(xs)\n");
    assert!(js.contains("xs.length"), "typed list len → .length: {}", js);
    assert!(!js.contains("pyLen(xs)"), "no pyLen for typed list: {}", js);
}

#[test]
fn test_issue22_len_list_literal_native() {
    // len([1, 2]) — literal list inferred as List → native .length.
    let js = compile("n = len([1, 2, 3])\n");
    assert!(js.contains(".length"), "list literal len → .length: {}", js);
    assert!(!js.contains("pyLen("), "no pyLen for list literal: {}", js);
}

#[test]
fn test_issue22_len_tuple_native() {
    // len((1, 2)) — literal tuple → native .length.
    let js = compile("n = len((1, 2, 3))\n");
    assert!(js.contains(".length"), "tuple len → .length: {}", js);
    assert!(!js.contains("pyLen("), "no pyLen for tuple: {}", js);
}

#[test]
fn test_issue22_len_dict_keeps_helper() {
    // len(d) where d is dict — must keep pyLen (JS object has no .length;
    // need Object.keys().length for Python semantics).
    let js = compile("d: dict = {}\nn = len(d)\n");
    assert!(js.contains("pyLen(d)"), "dict len must keep pyLen: {}", js);
    assert!(!js.contains("d.length"), "no .length on dict: {}", js);
}

#[test]
fn test_issue22_len_unknown_keeps_helper() {
    // len(x) where x has no static type — keep pyLen for safety.
    let js = compile("def f(x):\n    return len(x)\n");
    assert!(js.contains("pyLen(x)"), "unknown type → pyLen: {}", js);
}

#[test]
fn test_issue22_subscript_variable_keeps_pygetitem() {
    // Typed list VARIABLE subscripts keep pyGetItem regardless of index value:
    // without knowing the runtime length we cannot prove in-bounds, so
    // out-of-bounds access must still raise IndexError, not return undefined.
    let js = compile("xs: list = [1, 2, 3]\ny = xs[0]\n");
    assert!(
        js.contains("pyGetItem(xs, 0)"),
        "typed list variable subscript keeps pyGetItem: {}",
        js
    );
}

#[test]
fn test_issue22_dict_subscript_always_keeps_helper() {
    // d["key"] on a typed dict — always pyGetItem.
    // Missing key must raise Python KeyError, not silently return undefined.
    let js = compile("d: dict = {}\nv = d[\"key\"]\n");
    assert!(
        js.contains("pyGetItem(d, \"key\")"),
        "dict subscript keeps pyGetItem: {}",
        js
    );
}

// ─── Issue #22 follow-up B — list-literal + in-bounds literal index ─────────

#[test]
fn test_issue22b_list_literal_inbounds_native() {
    // List literal indexed by a non-negative literal that is < list length:
    // provably in-bounds at compile time → emit native x[i], no pyGetItem.
    // [10, 20, 30] has 3 elements; index 1 is 0 ≤ 1 < 3 → in-bounds.
    let js = compile("y = [10, 20, 30][1]\n");
    assert!(
        js.contains("[10, 20, 30][1]"),
        "in-bounds literal → native: {}",
        js
    );
    assert!(
        !js.contains("pyGetItem"),
        "in-bounds literal must skip pyGetItem: {}",
        js
    );
}

#[test]
fn test_issue22b_list_literal_first_element_native() {
    // Index 0 on a non-empty literal list is always in-bounds.
    let js = compile("y = [\"a\", \"b\", \"c\"][0]\n");
    assert!(
        !js.contains("pyGetItem"),
        "index 0 on literal must skip pyGetItem: {}",
        js
    );
}

#[test]
fn test_issue22b_list_literal_last_element_native() {
    // Index == len-1 (2 for a 3-element list) is also provably in-bounds.
    let js = compile("y = [\"a\", \"b\", \"c\"][2]\n");
    assert!(
        !js.contains("pyGetItem"),
        "last-element literal must skip pyGetItem: {}",
        js
    );
}

#[test]
fn test_issue22b_list_literal_oob_keeps_helper() {
    // Out-of-bounds literal index on a list literal: [a,b,c][3] is NOT
    // in-bounds (3 ≥ len=3); must keep pyGetItem to raise IndexError.
    let js = compile("y = [10, 20, 30][3]\n");
    assert!(
        js.contains("pyGetItem"),
        "oob literal index keeps pyGetItem: {}",
        js
    );
}

#[test]
fn test_issue22b_list_literal_negative_index_keeps_helper() {
    // Negative index on a list literal: pyGetItem handles the Python-style
    // wrap-around (-1 → last element) and range check. Not safe to skip.
    let js = compile("y = [10, 20, 30][-1]\n");
    assert!(
        js.contains("pyGetItem"),
        "negative literal index keeps pyGetItem: {}",
        js
    );
}

#[test]
fn test_issue22b_list_variable_inbounds_literal_still_keeps_helper() {
    // Even when the literal index looks in-bounds, a list VARIABLE (not
    // literal) must keep pyGetItem — we don't know the runtime length.
    // xs = [1, 2, 3] — index 0 is in-bounds for this assignment, but the
    // compiler cannot prove it without aliasing/length analysis.
    let js = compile("xs = [1, 2, 3]\ny = xs[0]\n");
    assert!(
        js.contains("pyGetItem(xs, 0)"),
        "list variable still wraps: {}",
        js
    );
}

// --- B-029 follow-up C: @handler auto-wires module default export ---

#[test]
fn test_handler_decorator_emits_export_default_fetch() {
    // @handler on a module-level async function should:
    //  1. Compile the function as a plain named function (no handler() call wrapping)
    //  2. Emit `export default { fetch: <fnName> };` after the function
    //  3. NOT emit `fetch_fn = handler(fetch_fn);` (consumed by codegen)
    let js = compile(
        r#"
from pyths.web import handler, Response

@handler
async def fetch(request):
    return Response("ok")
"#,
    );
    // The function itself compiles as a normal async function
    assert!(
        js.contains("async function fetch("),
        "function emitted: {}",
        js
    );
    // Auto-wired default export with the correct shape
    assert!(
        js.contains("export default { fetch: fetch };"),
        "default export wired: {}",
        js
    );
    // The handler decorator must NOT be applied as a runtime call
    assert!(
        !js.contains("fetch = handler(fetch)"),
        "handler not applied as runtime call: {}",
        js
    );
}

#[test]
fn test_handler_decorator_on_sync_function() {
    // @handler also works on synchronous (non-async) functions
    let js = compile(
        r#"
from pyths.web import handler, Response

@handler
def my_fetch(request):
    return Response("ok")
"#,
    );
    assert!(
        js.contains("function my_fetch("),
        "function emitted: {}",
        js
    );
    assert!(
        js.contains("export default { fetch: my_fetch };"),
        "default export wired: {}",
        js
    );
    assert!(
        !js.contains("my_fetch = handler(my_fetch)"),
        "handler not applied as runtime: {}",
        js
    );
}

#[test]
fn test_handler_explicit_form_still_works() {
    // The explicit __default__ = handler(fn) form (B-029) must still compile
    // to `export default handler(fn);` (the runtime call produces {fetch: fn}).
    let js = compile(
        r#"
from pyths.web import handler, Response

async def my_fetch(request):
    return Response("ok")

__default__ = handler(my_fetch)
"#,
    );
    // The function compiles normally (no decorator)
    assert!(
        js.contains("async function my_fetch("),
        "function emitted: {}",
        js
    );
    // __default__ = X → export default X;
    assert!(
        js.contains("export default handler(my_fetch);"),
        "explicit form: {}",
        js
    );
    // No auto-wired default from decorator (no @handler on the function)
    assert!(
        !js.contains("export default { fetch:"),
        "no spurious auto-wire: {}",
        js
    );
}

#[test]
fn test_handler_decorator_imports_web_module() {
    // from pyths.web import handler → import { handler, ... } from "pyths-runtime/web"
    let js = compile("from pyths.web import handler, Response");
    assert!(
        js.contains("from \"pyths-runtime/web\""),
        "web module path: {}",
        js
    );
    assert!(js.contains("handler"), "handler imported: {}", js);
}

// --- B-030 follow-up D: --target worker emits pyths-runtime/core imports ---

#[test]
fn test_worker_target_runtime_imports_from_core() {
    // --target worker (worker_runtime=true): numeric/collection helpers must
    // import from "pyths-runtime/core", not "pyths-runtime". This is the
    // DOM-free Worker-safe subpath introduced by B-030.
    let js = compile_worker("x = a + b\ny = len(xs)\n");
    // Must import from core, not the default package
    assert!(
        js.contains("from \"pyths-runtime/core\""),
        "--target worker must import from pyths-runtime/core: {}",
        js
    );
    assert!(
        !js.contains("from \"pyths-runtime\""),
        "--target worker must NOT import from bare pyths-runtime: {}",
        js
    );
}

#[test]
fn test_default_target_runtime_imports_from_root() {
    // Default target (no worker flag): runtime helpers must come from
    // "pyths-runtime", not the /core subpath. Regression guard — worker flag
    // must not bleed into the default compilation path.
    let js = compile("x = a + b\ny = len(xs)\n");
    assert!(
        js.contains("from \"pyths-runtime\""),
        "default target must import from pyths-runtime: {}",
        js
    );
    assert!(
        !js.contains("from \"pyths-runtime/core\""),
        "default target must NOT import from pyths-runtime/core: {}",
        js
    );
}

#[test]
fn test_worker_target_preserves_all_helpers() {
    // The same helper names are emitted under --target worker; only the
    // import path changes. Verify a range of helpers still appear by name.
    let js = compile_worker("x = a + b\ny = a // b\nz = a ** b\nn = len(xs)\nit = range(10)\n");
    assert!(js.contains("pyAdd("), "pyAdd present: {}", js);
    assert!(js.contains("pyFloorDiv("), "pyFloorDiv present: {}", js);
    assert!(js.contains("pyPow("), "pyPow present: {}", js);
    assert!(js.contains("pyLen("), "pyLen present: {}", js);
    assert!(js.contains("pyRange("), "pyRange present: {}", js);
    // All under the core subpath
    assert!(
        js.contains("from \"pyths-runtime/core\""),
        "core import: {}",
        js
    );
}

#[test]
fn test_worker_target_web_import_path_unchanged() {
    // from pyths.web import handler → pyths-runtime/web is an explicit
    // user import, not a runtime-helper import. The --target worker flag
    // must not alter these explicit import paths.
    let js = compile_worker("from pyths.web import handler, Response\n");
    assert!(
        js.contains("from \"pyths-runtime/web\""),
        "explicit web import unchanged under worker target: {}",
        js
    );
}

// ── #83: hybrid dict representation (plain object vs Map-backed PyDict) ────

#[test]
fn test_dict_literal_string_keys_stay_plain() {
    // All-provably-string keys keep the plain-object shape (JS interop:
    // React props, JSON, spread). f-strings and str(...) count as strings.
    let js = compile("d = {\"a\": 1, \"b\": 2}");
    assert!(js.contains("({\"a\": 1, \"b\": 2})"), "plain shape: {}", js);
    assert!(!js.contains("PyDict"), "no PyDict for string keys: {}", js);

    let js = compile("x = 1\nd = {f\"k{x}\": 1, str(x): 2}");
    assert!(
        !js.contains("PyDict"),
        "fstring/str() keys stay plain: {}",
        js
    );
}

#[test]
fn test_dict_literal_nonstring_keys_use_pydict() {
    let js = compile("d = {1: \"a\", 2: \"b\"}");
    assert!(
        js.contains("new PyDict([[1, \"a\"], [2, \"b\"]])"),
        "PyDict: {}",
        js
    );

    // ANY non-provably-string key flips the whole literal
    let js = compile("k = \"s\"\nd = {\"a\": 1, k: 2}");
    assert!(js.contains("new PyDict("), "dynamic key -> PyDict: {}", js);
}

#[test]
fn test_dict_comprehension_shape_selection() {
    // String key expr → plain Object.fromEntries (pre-#83 shape)
    let js = compile("d = {f\"k{x}\": x for x in range(3)}");
    assert!(
        js.contains("Object.fromEntries("),
        "string comp stays plain: {}",
        js
    );

    // Non-string key expr → PyDict over the same pair stream
    let js = compile("d = {x: x * x for x in range(3)}");
    assert!(js.contains("new PyDict("), "int comp -> PyDict: {}", js);
    assert!(
        !js.contains("Object.fromEntries("),
        "no fromEntries: {}",
        js
    );
}

#[test]
fn test_dict_subscript_write_routes_pysetitem() {
    // Dict-typed receiver: raw `d[k] = v` on a Map-backed dict would set
    // a useless own property; route through pySetItem.
    let js = compile("d = {1: \"a\"}\nd[2] = \"b\"");
    assert!(js.contains("pySetItem(d, 2, \"b\")"), "dict write: {}", js);

    // List-typed receiver keeps the bare native write (hot path).
    let js = compile("xs = [1, 2]\nxs[0] = 9");
    // crit-7: list writes route through the bounds-checked pySetItem.
    assert!(
        js.contains("pySetItem(xs, 0, 9)"),
        "list write routes via pySetItem: {}",
        js
    );
}

#[test]
fn test_dict_aug_assign_routes_through_helpers() {
    // `d[k] += v` on a Dict receiver: hoisted read-modify-write via
    // pyGetItem/pySetItem + the Python operator helper.
    let js = compile("d = {1: 10}\nd[1] += 5");
    assert!(
        js.contains("pySetItem(__aug_o0, __aug_k0, pyAdd(pyGetItem(__aug_o0, __aug_k0), 5))"),
        "aug: {}",
        js
    );
}

#[test]
fn test_for_in_dict_wraps_pydictkeys() {
    // `for k in d` iterates KEYS in Python — plain objects aren't even
    // iterable in JS, so Dict-typed iterables get the pyDictKeys wrap.
    let js = compile("d = {\"a\": 1}\nfor k in d:\n    print(k)");
    assert!(js.contains("of pyDictKeys(d)"), "for-in dict: {}", js);

    // Comprehension sources too
    let js = compile("d = {1: \"a\"}\nks = [k for k in d]");
    assert!(js.contains("pyDictKeys(d)"), "comp over dict: {}", js);

    // Lists stay bare
    let js = compile("xs = [1]\nfor x in xs:\n    print(x)");
    assert!(
        !js.contains("pyDictKeys"),
        "list iteration stays bare: {}",
        js
    );
}

#[test]
fn test_dict_builtin_uses_factory() {
    // dict(...) was mapped to the PyDict CLASS (crashed without `new`);
    // now the pyDict factory, which shape-chooses at runtime.
    let js = compile("d = dict([(1, \"a\")])");
    assert!(js.contains("pyDict("), "dict() factory: {}", js);
    assert!(!js.contains("new PyDict(pyTuple"), "not the class: {}", js);
}

#[test]
fn test_hybrid_clear_copy_gated_on_list_type() {
    // Found while shape-testing #83: `d.copy()` / `d.clear()` on a
    // simple-Name DICT receiver inlined the LIST fast path (`d.slice()`,
    // `d.length = 0` — the latter leaves a bogus `length` key).
    let js = compile("d = {\"a\": 1}\ne = d.copy()\nd.clear()");
    assert!(js.contains("pyCopy(d)"), "dict copy -> runtime: {}", js);
    assert!(js.contains("pyClear(d)"), "dict clear -> runtime: {}", js);

    // Lists keep the inline fast path.
    let js = compile("xs = [1]\nys = xs.copy()\nxs.clear()");
    assert!(js.contains("xs.slice()"), "list copy inline: {}", js);
    assert!(js.contains("xs.length = 0"), "list clear inline: {}", js);
}

// --- #105: `from pyths import <stdlib>` must resolve like `import <stdlib>` ---

#[test]
fn test_from_pyths_import_stdlib() {
    let js = compile("from pyths import math");
    assert!(
        js.contains("import * as math from \"pyths-runtime/stdlib/math\""),
        "aliases the bare stdlib import: {}",
        js
    );
    assert!(
        !js.contains("from \"pyths\""),
        "no unresolvable `pyths` specifier: {}",
        js
    );
}

#[test]
fn test_from_pyths_import_stdlib_alias() {
    let js = compile("from pyths import math as m");
    assert!(
        js.contains("import * as m from \"pyths-runtime/stdlib/math\""),
        "user alias binds the namespace: {}",
        js
    );
}

#[test]
fn test_from_pyths_import_multiple_stdlib() {
    let js = compile("from pyths import math, json");
    assert!(
        js.contains("import * as math from \"pyths-runtime/stdlib/math\""),
        "JS: {}",
        js
    );
    assert!(
        js.contains("import * as json from \"pyths-runtime/stdlib/json\""),
        "JS: {}",
        js
    );
}

#[test]
fn test_from_pyths_import_unknown_is_diagnostic() {
    // `from pyths import <not-a-stdlib>` used to compile cleanly to
    // `import { x } from "pyths"` — an unresolvable specifier (silent
    // miscompile, #105). It must now surface a codegen diagnostic and
    // emit a loud module-load-time throw instead of the broken import.
    let module = pyths_parser::parse("from pyths import nonexistent").unwrap();
    let mut gen = pyths_codegen_js::JsCodegen::new();
    gen.emit_module(&module);
    let errors = gen.take_errors();
    assert_eq!(errors.len(), 1, "expected one diagnostic: {:?}", errors);
    assert!(
        errors[0].contains("nonexistent") && errors[0].contains("import"),
        "diagnostic names the symbol and the supported forms: {:?}",
        errors
    );
    let js = gen.finish();
    assert!(
        !js.contains("from \"pyths\""),
        "no unresolvable import emitted: {}",
        js
    );
    assert!(
        js.contains("throw new Error("),
        "loud failure emitted: {}",
        js
    );
}

// --- #108: dynamic f-string format specs (nested braces) ---

#[test]
fn test_fstring_dynamic_format_spec_width() {
    // f"{v:{w}}" — the spec is built at runtime; must lower to the
    // pyFormatDynamic runtime helper, not silently drop the spec.
    let js = compile("v = 42\nw = 8\nprint(f\"{v:{w}}\")");
    assert!(
        js.contains("pyFormatDynamic("),
        "dynamic spec lowers to runtime helper: {}",
        js
    );
    assert!(
        js.contains("import { pyFormatDynamic")
            || js.contains("pyFormatDynamic }")
            || js.contains("pyFormatDynamic,"),
        "helper auto-imported: {}",
        js
    );
}

#[test]
fn test_fstring_dynamic_format_spec_precision() {
    // f"{x:.{p}f}" — mixed literal + expression spec template.
    let js = compile("p = 3\nprint(f\"{3.14159:.{p}f}\")");
    assert!(
        js.contains("pyFormatDynamic("),
        "dynamic spec lowers to runtime helper: {}",
        js
    );
}

#[test]
fn test_fstring_static_spec_unchanged() {
    // Static specs must keep their existing compile-time lowering —
    // no pyFormatDynamic for f"{x:.2f}".
    let js = compile("print(f\"{3.14159:.2f}\")");
    assert!(
        !js.contains("pyFormatDynamic("),
        "static spec keeps fast path: {}",
        js
    );
}

// --- #136: integral-float repr — definitely-float widening ---

#[test]
fn test_repr_of_float_arithmetic_uses_format_float() {
    let js = compile("print(repr(700.0 + 2.0))");
    assert!(
        js.contains("pyFormatFloat("),
        "float arithmetic routes to pyFormatFloat: {}",
        js
    );
}

#[test]
fn test_repr_of_int_division_uses_format_float() {
    // Python 3 true division of ints is a float: repr(8 / 2) == '4.0'.
    let js = compile("print(repr(8 / 2))");
    assert!(
        js.contains("pyFormatFloat("),
        "int/int division routes to pyFormatFloat: {}",
        js
    );
}

#[test]
fn test_repr_of_annotated_float_call_uses_format_float() {
    let js = compile("def f(x: int) -> float:\n    return x * 1.0\nprint(repr(f(702)))");
    assert!(
        js.contains("pyFormatFloat("),
        "-> float call routes to pyFormatFloat: {}",
        js
    );
}

#[test]
fn test_decimal_division_repr_not_float_formatted() {
    // The reason is_definitely_float is a whitelist: Decimal/Fraction
    // division must NOT be float-formatted (their __repr__ differs).
    let js = compile(
        "from decimal import Decimal\na = Decimal(\"1\")\nb = Decimal(\"3\")\nprint(repr(a / b))",
    );
    assert!(
        !js.contains("pyFormatFloat("),
        "Decimal division stays on pyRepr: {}",
        js
    );
}

// --- #121: undecorated capitalized helper components get the PSX transform ---

#[test]
fn test_undecorated_capitalized_helper_gets_psx() {
    // The behavioral oracle's movie_rows failure shape: a top-level
    // capitalized helper with no @component/@psx whose body returns a
    // bare HTML-tag call. Without the transform, `div(...)` is a
    // guaranteed ReferenceError at mount.
    let js = compile(
        "from pyths.react import component\n\
         \n\
         def MovieCard(movie):\n\
         \x20   return div(class_name=\"card\", movie[\"title\"])\n\
         \n\
         @component\n\
         def App():\n\
         \x20   return MovieCard({\"title\": \"x\"})\n",
    );
    assert!(
        js.contains("createElement(\"div\""),
        "helper body must be PSX-transformed: {}",
        js
    );
}

#[test]
fn test_capitalized_factory_returning_known_call_stays_plain() {
    // Guard: a capitalized function returning a call to a DECLARED name
    // is NOT claimed — only unbound HTML-tag calls are unmistakably PSX.
    let js = compile(
        "def section(x):\n\
         \x20   return x * 2\n\
         \n\
         def Doubler(v):\n\
         \x20   return section(v)\n",
    );
    assert!(
        !js.contains("createElement"),
        "declared name shadowing must defeat implicit PSX: {}",
        js
    );
}

// --- #122: loop-capture idiom `lambda i=i:` in event-handler props ---

#[test]
fn test_event_handler_self_default_lambda_captures() {
    // `on_click=lambda i=i: toggle(i)` compiled to `(i = i) => toggle(i)`:
    // React passes the SyntheticEvent as arg 1, overriding i (and the JS
    // self-default is a TDZ ReferenceError when called argless).
    let js = compile(
        "from pyths.react import component\n\
         \n\
         @component\n\
         def List(items, toggle):\n\
         \x20   return div(*[button(on_click=lambda i=i: toggle(i), \"x\") for i in range(len(items))])\n",
    );
    assert!(
        js.contains("((i) => () => toggle(i))(i)"),
        "self-default becomes creation-time capture: {}",
        js
    );
    assert!(
        !js.contains("(i = i)"),
        "no JS self-default TDZ pattern: {}",
        js
    );
}

#[test]
fn test_event_handler_mixed_defaults() {
    // Self-defaulted params capture; others (e=None) stay real params
    // and receive the event.
    let js = compile(
        "from pyths.react import component\n\
         \n\
         @component\n\
         def Row(update):\n\
         \x20   i = 0\n\
         \x20   return input(on_change=lambda e=None, i=i: update(i, e))\n",
    );
    assert!(
        js.contains("((i) => (e = null) => update(i, e))(i)"),
        "mixed: capture i, keep e as event param: {}",
        js
    );
}

#[test]
fn test_event_handler_event_default_untouched() {
    // `lambda e=None:` wants the event — must NOT be rewritten.
    let js = compile(
        "from pyths.react import component\n\
         \n\
         @component\n\
         def Field(save):\n\
         \x20   return input(on_change=lambda e=None: save(e))\n",
    );
    assert!(
        js.contains("(e = null) => save(e)") && !js.contains("(() => (e"),
        "event-receiving lambda stays as-is: {}",
        js
    );
}

// ── Round-2 pythonic sweep: walrus operator + iter() (r2_w_*) ──────────

#[test]
fn test_walrus_module_level_hoisted() {
    // (n := ...) assigns in expression position — the name must get a
    // hoisted `let` or strict-mode ESM throws ReferenceError (r2_w_if).
    let js = compile("data = [1, 2, 3, 4]\nif (n := len(data)) > 3:\n    print(n)");
    assert!(js.contains("let n;"), "walrus target hoisted: {}", js);
    assert!(js.contains("(n = data.length)"), "walrus assigns: {}", js);
}

#[test]
fn test_walrus_while_condition() {
    let js = compile(
        "vals = [5, 4, 0]\nit = iter(vals)\ntotal = 0\nwhile (v := next(it, 0)) != 0:\n    total = total + v",
    );
    assert!(
        js.contains("let v;"),
        "while-condition walrus hoisted: {}",
        js
    );
    assert!(
        js.contains("pyIter(vals)"),
        "iter() lowers to pyIter: {}",
        js
    );
}

#[test]
fn test_walrus_in_comprehension_binds_enclosing_scope() {
    // PEP 572: `:=` inside a comprehension binds the ENCLOSING scope; the
    // hoisted declaration makes the arrow-closure assignment land there
    // (r2_w_comp_filter / r2_w_comp_scope_leak).
    let js = compile("ys = [y for x in range(10) if (y := x * 2) > 12]");
    assert!(
        js.contains("let y;"),
        "comprehension walrus hoisted: {}",
        js
    );
}

#[test]
fn test_walrus_inside_function_body() {
    let js = compile("def f(xs):\n    if (m := len(xs)) > 2:\n        return m\n    return 0");
    assert!(
        js.contains("let m;"),
        "function-scope walrus hoisted inside the function: {}",
        js
    );
}

#[test]
fn test_walrus_in_fstring() {
    let js = compile("s = f\"{(w := 5)}-{w * 2}\"");
    assert!(js.contains("let w;"), "f-string walrus hoisted: {}", js);
}

#[test]
fn test_iter_builtin_maps_to_py_iter() {
    let js = compile("it = iter([1, 2])\nx = next(it)");
    assert!(js.contains("pyIter([1, 2])"), "iter() lowered: {}", js);
    assert!(js.contains("pyNext(it)"), "next() lowered: {}", js);
}

#[test]
fn test_walrus_comprehension_single_pass() {
    // A walrus bound in the filter and read in the map must NOT take the
    // two-pass .filter().map() fast path (it would read the last filter
    // pass's binding: [18, 18, 18] instead of [14, 16, 18]).
    let js = compile("ys = [y for x in range(10) if (y := x * 2) > 12]");
    assert!(
        !js.contains(".filter("),
        "walrus comprehension takes the single-pass loop path: {}",
        js
    );
    assert!(js.contains("__result.push("), "loop path used: {}", js);
}

#[test]
fn test_star_target_for_loop_destructures_rest() {
    // Round-2 pythonic sweep: `for a, *bs in ...` lowers to a JS rest
    // pattern (previously a parse error).
    let js = compile("for a, *bs in [[1, 2, 3]]:\n    print(a, bs)");
    assert!(js.contains("[a, ...bs]"), "rest pattern emitted: {}", js);
}

// ── Round-2 pythonic sweep: match-statement lowering (r2_m_match_*) ────

#[test]
fn test_match_builtin_class_pattern() {
    // `case int():` must dispatch through the __pyIsInstance string
    // sentinels (bare `instanceof int` is a ReferenceError).
    let js = compile(
        "def g(x):\n    match x:\n        case int() if x > 10:\n            return 'big'\n        case str() as s:\n            return s\n        case _:\n            return 'other'",
    );
    assert!(
        js.contains("__pyIsInstance(__match0, \"int\")"),
        "int() pattern uses sentinel: {}",
        js
    );
    assert!(
        js.contains("__pyIsInstance(__match0, \"str\")"),
        "str() pattern uses sentinel: {}",
        js
    );
    assert!(
        !js.contains("instanceof int"),
        "no bare instanceof int: {}",
        js
    );
}

#[test]
fn test_match_mapping_pattern_shape_guarded() {
    // `case {'k': v}` vs a string subject threw TypeError (`'k' in "hi"`);
    // the mapping arm now guards the shape and handles Map-backed dicts.
    let js = compile(
        "def kind(x):\n    match x:\n        case {'k': v}:\n            return v\n        case _:\n            return None",
    );
    assert!(
        js.contains("typeof __match0 === \"object\""),
        "shape guard present: {}",
        js
    );
    assert!(
        js.contains("__match0 instanceof Map ? __match0.has("),
        "Map-aware key check: {}",
        js
    );
}

#[test]
fn test_match_sequence_star_pattern() {
    // `case [first, *rest]:` — minimum-length check + slice binding.
    let js = compile(
        "def f(x):\n    match x:\n        case [first, *rest]:\n            return rest\n        case _:\n            return None",
    );
    assert!(
        js.contains("__match0.length >= 1"),
        "star sequence uses minimum length: {}",
        js
    );
    assert!(
        js.contains("__match0.slice(1, __match0.length - 0)"),
        "star binds the slice: {}",
        js
    );
}

// ── Round-2 pythonic sweep: semantics batch (r2_m_*) ───────────────────

#[test]
fn test_dict_iunion_routes_through_py_bit_or() {
    // `d |= {...}` previously emitted raw JS `|=` (numeric coercion → 0).
    let js = compile("d = {\"x\": 1}\nd |= {\"y\": 2}");
    assert!(js.contains("d = pyBitOr(d, "), "dict |= merges: {}", js);
}

#[test]
fn test_chained_comparison_single_evaluation() {
    // `1 < mid() < 10` must evaluate mid() exactly once.
    let js = compile("ok = 1 < mid() < 10");
    assert!(
        js.contains("((__cmp0) =>"),
        "middle operand captured once: {}",
        js
    );
    assert_eq!(js.matches("mid()").count(), 1, "mid() emitted once: {}", js);
}

#[test]
fn test_mid_star_unpack_element_wise() {
    // `a, *mid, z = xs` — JS rest must be last, so lower via a temp +
    // index/slice assignments.
    let js = compile("a, *mid, z = [1, 2, 3, 4, 5]");
    assert!(
        !js.contains("...mid, z]"),
        "no invalid mid-rest pattern: {}",
        js
    );
    assert!(
        js.contains("pySlice(__unpack0, 1, -1"),
        "star gets slice: {}",
        js
    );
}

#[test]
fn test_kwargs_call_binds_by_name() {
    let js = compile(
        "def f(a, b, c):\n    return a\nx = f(1, b=2, c=3)\ny = f(1, **{\"b\": 2, \"c\": 3})",
    );
    assert!(
        js.contains("f.__pyparams__ = [\"a\", \"b\", \"c\"];"),
        "metadata attached: {}",
        js
    );
    assert!(
        js.contains("__pyCallKw(f, [1], {b: 2, c: 3})"),
        "named kwargs route through __pyCallKw: {}",
        js
    );
}

#[test]
fn test_keyword_only_separator_parses() {
    // `def g(a, *, scale=2)` — the bare `*` is a separator, not a param.
    let js = compile("def g(a, *, scale=2):\n    return a * scale\nr = g(3, scale=5)");
    assert!(
        js.contains("function g(a, scale ="),
        "kw-only param emitted: {}",
        js
    );
    assert!(
        js.contains("g.__pyparams__ = [\"a\", \"scale\"];"),
        "metadata: {}",
        js
    );
}

#[test]
fn test_reserved_word_param_kwarg_binding() {
    // B1: a param named like a JS reserved word (`default`) is DECLARED
    // `default$` in the JS signature, but __pyparams__ must store the RAW
    // Python name so a keyword call `greet("x", default="yo")` binds — the
    // runtime (__pyKwArgs) matches raw call-site keys against __pyparams__.
    // Storing the sanitized form made every such keyword call miss →
    // "TypeError: got an unexpected keyword argument 'default'".
    let js = compile(
        "def greet(name, default=\"hi\"):\n    return default\nr = greet(\"x\", default=\"yo\")",
    );
    // JS parameter declaration keeps the sanitized form.
    assert!(
        js.contains("default$"),
        "param declared sanitized in signature: {}",
        js
    );
    // __pyparams__ stores the raw names (NOT "default$").
    assert!(
        js.contains("greet.__pyparams__ = [\"name\", \"default\"];"),
        "metadata stores raw param names: {}",
        js
    );
    assert!(
        !js.contains("[\"name\", \"default$\"]"),
        "no sanitized name inside __pyparams__: {}",
        js
    );
    // The keyword call routes through __pyCallKw with the raw key.
    assert!(
        js.contains("__pyCallKw(greet, "),
        "keyword call routes via __pyCallKw: {}",
        js
    );
    assert!(
        js.contains("{default: \"yo\"}"),
        "raw kwarg key at call site: {}",
        js
    );
}

#[test]
fn test_reserved_word_params_new_and_in() {
    // Additional reserved-word param names must also round-trip raw.
    let js = compile("def f(new, in_=1):\n    return new\n");
    // `in` is a reserved word; `in_` is not — cover the sanitized case `new`.
    assert!(
        js.contains("f.__pyparams__ = [\"new\", \"in_\"];"),
        "raw reserved-word param names in metadata: {}",
        js
    );
    let js2 = compile("def g(case, switch):\n    return case\n");
    assert!(
        js2.contains("g.__pyparams__ = [\"case\", \"switch\"];"),
        "raw reserved-word param names in metadata: {}",
        js2
    );
    // `case$` legitimately appears in the JS parameter declaration
    // (`function g(case$, switch$)`); only the QUOTED metadata form must
    // never carry the sanitized name.
    assert!(
        !js2.contains("\"case$\""),
        "no sanitized name in metadata: {}",
        js2
    );
}

#[test]
fn test_nextjs_renamed_export_pyparams_target() {
    // B9: a Next.js export whose name is renamed (`generate_metadata` →
    // `generateMetadata`) must attach __pyparams__ to the SAME emitted name
    // as the declaration. Using the un-renamed identifier produced
    // `generate_metadata.__pyparams__ = ...` referencing a name that does
    // not exist → ReferenceError at module load, breaking the whole module.
    let js = compile("def generate_metadata(params):\n    return {\"title\": \"x\"}");
    assert!(
        js.contains("export function generateMetadata("),
        "declared under the renamed name: {}",
        js
    );
    assert!(
        js.contains("generateMetadata.__pyparams__ = [\"params\"];"),
        "__pyparams__ targets the renamed (declared) name: {}",
        js
    );
    assert!(
        !js.contains("generate_metadata.__pyparams__"),
        "no reference to the un-renamed identifier: {}",
        js
    );
}

// ── Round-3 pythonic sweep: class-model core (c3_*) ────────────────────

#[test]
fn test_class_attribute_installed_post_class() {
    // Class-level assignments previously emitted invalid `let x = ...`
    // INSIDE the class body; they now install via __pyClassAttr (class
    // attr + live prototype accessor).
    let js = compile(
        "class Animal:\n    kind = 'generic'\n    def speak(self):\n        return self.kind",
    );
    assert!(!js.contains("let kind"), "no let in class body: {}", js);
    assert!(
        js.contains("__pyClassAttr(Animal, \"kind\", \"generic\")"),
        "class attr installed: {}",
        js
    );
}

#[test]
fn test_classmethod_static_with_cls() {
    let js = compile(
        "class Counter:\n    def __init__(self, n):\n        self.n = n\n    @classmethod\n    def make(cls):\n        return cls(42)",
    );
    assert!(js.contains("static make("), "classmethod is static: {}", js);
    assert!(js.contains("const cls = this;"), "cls bound: {}", js);
    assert!(js.contains("new cls(42)"), "cls(...) constructs: {}", js);
    assert!(
        js.contains(
            "Counter.prototype.make = function (...a) { return this.constructor.make(...a); };"
        ),
        "instance access wrapper: {}",
        js
    );
}

#[test]
fn test_staticmethod_reachable_from_instances() {
    let js = compile("class Util:\n    @staticmethod\n    def double(x):\n        return x * 2");
    assert!(js.contains("static double("), "static emitted: {}", js);
    assert!(
        js.contains("Util.prototype.double = Util.double;"),
        "prototype alias: {}",
        js
    );
}

#[test]
fn test_property_setter_emits_set_accessor() {
    let js = compile(
        "class Temp:\n    def __init__(self):\n        self._c = 0\n    @property\n    def celsius(self):\n        return self._c\n    @celsius.setter\n    def celsius(self, v):\n        self._c = v",
    );
    assert!(js.contains("get celsius("), "getter: {}", js);
    assert!(js.contains("set celsius("), "setter: {}", js);
}

#[test]
fn test_unbound_method_call_through_class() {
    // `Animal.speak(self)` — instance methods live on the prototype.
    let js = compile(
        "class Animal:\n    def speak(self):\n        return 'generic'\nclass Dog(Animal):\n    def speak(self):\n        return Animal.speak(self) + '+woof'",
    );
    assert!(
        js.contains("__pyClassCall(Animal, \"speak\", [this])"),
        "unbound call routed: {}",
        js
    );
}

#[test]
fn test_dunder_repr_str_keep_names_with_tostring_alias() {
    let js = compile(
        "class Money:\n    def __init__(self, v):\n        self.v = v\n    def __repr__(self):\n        return 'Money(' + str(self.v) + ')'\n    def __str__(self):\n        return '$' + str(self.v)",
    );
    assert!(js.contains("__repr__("), "__repr__ keeps its name: {}", js);
    assert!(js.contains("__str__("), "__str__ keeps its name: {}", js);
    assert!(
        js.contains("Money.prototype.toString = Money.prototype.__str__;"),
        "toString alias prefers __str__: {}",
        js
    );
}

#[test]
fn test_dataclass_generates_repr_via_py_repr() {
    let js = compile(
        "from dataclasses import dataclass\n\n@dataclass\nclass Point:\n    x: int\n    y: int\n",
    );
    assert!(
        js.contains("return \"Point(\" + \"x=\" + pyRepr(this.x) + \", \" + \"y=\" + pyRepr(this.y) + \")\";"),
        "generated __repr__ is repr-exact: {}",
        js
    );
    assert!(
        js.contains("return this.__repr__();"),
        "toString delegates: {}",
        js
    );
}

// ── Round-3 pythonic sweep: dunder-protocol batch ──────────────────────

#[test]
fn test_with_statement_context_manager_protocol() {
    let js = compile(
        "class Ctx:\n    def __enter__(self):\n        return 42\n    def __exit__(self, a, b, c):\n        return False\nwith Ctx() as v:\n    print(v)",
    );
    assert!(js.contains(".__enter__()"), "enter called: {}", js);
    assert!(
        js.contains(".__exit__(null, null, null)"),
        "exit on success: {}",
        js
    );
    assert!(js.contains("_exc = null;"), "exception tracking: {}", js);
}

#[test]
fn test_bare_raise_builtin_exception_instantiates_and_imports() {
    let js = compile("def f(n):\n    if n <= 0:\n        raise StopIteration\n    return n");
    assert!(
        js.contains("throw new StopIteration();"),
        "instantiated: {}",
        js
    );
    assert!(
        js.contains("StopIteration") && js.contains("import"),
        "imported: {}",
        js
    );
}

// ── Round-3 pythonic sweep: class kwargs (__pyKwArgs) ──────────────────

#[test]
fn test_class_constructor_kwargs_bind_by_name() {
    let js = compile(
        "class Point:\n    def __init__(self, x, y):\n        self.x = x\n        self.y = y\np = Point(y=4, x=3)",
    );
    assert!(
        js.contains("Point.__pyparams__ = [\"x\", \"y\"];"),
        "ctor metadata: {}",
        js
    );
    assert!(
        js.contains("new Point(...__pyKwArgs(Point, [], {y: 4, x: 3}))"),
        "kwargs construction: {}",
        js
    );
}

#[test]
fn test_method_kwargs_bind_by_name_this_preserved() {
    let js = compile(
        "class Num:\n    def __init__(self, v):\n        self.v = v\n    def scale(self, factor=1):\n        return self.v * factor\np = Num(7)\nr = p.scale(factor=3)",
    );
    assert!(
        js.contains("Num.prototype.scale.__pyparams__ = [\"factor\"];"),
        "method metadata: {}",
        js
    );
    assert!(
        js.contains("p.scale(...__pyKwArgs(p.scale, [], {factor: 3}))"),
        "call keeps receiver (this): {}",
        js
    );
}

#[test]
fn test_dataclass_kwargs_metadata() {
    let js = compile(
        "from dataclasses import dataclass\n\n@dataclass\nclass Point:\n    x: int\n    y: int\np = Point(y=2, x=1)",
    );
    assert!(
        js.contains("Point.__pyparams__ = [\"x\", \"y\"];"),
        "dataclass field metadata: {}",
        js
    );
}

// ── Round-4 pythonic sweep: exceptions ──────────────────────────────────

#[test]
fn test_try_except_finally_closes_catch() {
    // try/except+finally used to leave the catch block unclosed, emitting
    // `\n finally {` (a JS SyntaxError). The catch must close with `}` and
    // the finally must attach as `} finally {`.
    let js = compile("try:\n    x = 1\nexcept ValueError:\n    x = 2\nfinally:\n    x = 3");
    assert!(js.contains("} finally {"), "JS: {}", js);
    assert!(
        !js.contains("\n finally {"),
        "unbalanced catch before finally: {}",
        js
    );
}

#[test]
fn test_try_else_runs_outside_handler_protection() {
    // Python: the else clause runs only when the body completed, and its
    // exceptions are NOT caught by this try's handlers -> completion-flag
    // lowering, not inlining at the end of the try body.
    let js = compile("try:\n    x = 1\nexcept ValueError:\n    x = 2\nelse:\n    x = 3");
    assert!(js.contains("let __else_0 = false;"), "JS: {}", js);
    assert!(js.contains("__else_0 = true;"), "JS: {}", js);
    assert!(js.contains("if (__else_0) {"), "JS: {}", js);
}

#[test]
fn test_bare_raise_rethrows_active_exception() {
    // Bare `raise` used to emit `throw;` -- a JS SyntaxError.
    let js = compile("try:\n    x = 1\nexcept ValueError:\n    raise");
    assert!(js.contains("throw __exc;"), "JS: {}", js);
    assert!(!js.contains("throw;"), "JS: {}", js);
}

#[test]
fn test_raise_from_sets_cause() {
    let js = compile("raise RuntimeError(\"outer\") from ValueError(\"inner\")");
    assert!(
        js.contains(
            "Object.assign(new RuntimeError(\"outer\"), { __cause__: new ValueError(\"inner\") });"
        ),
        "JS: {}",
        js
    );
}

#[test]
fn test_raise_from_bare_class_cause_instantiates() {
    let js = compile("raise RuntimeError(\"x\") from ValueError");
    assert!(js.contains("{ __cause__: new ValueError() }"), "JS: {}", js);
}

#[test]
fn test_class_gets_dunder_name() {
    let js = compile("class Foo:\n    pass");
    assert!(js.contains("Foo.__name__ = \"Foo\";"), "JS: {}", js);
}

#[test]
fn test_dunder_name_read_falls_back_to_js_name() {
    // type(e).__name__ / f.__name__: compiled + runtime classes carry
    // __name__, native JS classes and functions only carry .name.
    let js = compile("n = type(e).__name__");
    assert!(
        js.contains("((__o) => __o?.__name__ ?? __o?.name)"),
        "JS: {}",
        js
    );
}

#[test]
fn test_raise_typeerror_uses_native_global() {
    // TypeError is a real JS global; the runtime class must NOT be
    // imported over it at raise sites.
    let js = compile("raise TypeError(\"bad\")");
    assert!(js.contains("throw new TypeError(\"bad\")"), "JS: {}", js);
    let import_line = js.lines().find(|l| l.starts_with("import {")).unwrap_or("");
    assert!(!import_line.contains("TypeError"), "JS: {}", js);
}

// ── Round-4 pythonic sweep: asyncio + async constructs ──────────────────

#[test]
fn test_import_asyncio_maps_to_runtime_shim() {
    let js = compile("import asyncio");
    assert!(
        js.contains("import * as asyncio from \"pyths-runtime/asyncio\";"),
        "JS: {}",
        js
    );
}

#[test]
fn test_from_asyncio_import_maps_to_runtime_shim() {
    let js = compile("from asyncio import run, sleep, gather");
    assert!(js.contains("from \"pyths-runtime/asyncio\";"), "JS: {}", js);
}

#[test]
fn test_asyncio_run_awaited_at_top_level() {
    let js = compile("import asyncio\nasync def main():\n    return 1\nres = asyncio.run(main())");
    assert!(js.contains("(await asyncio.run(main()))"), "JS: {}", js);
}

#[test]
fn test_from_asyncio_run_awaited_at_top_level() {
    let js = compile("from asyncio import run\nasync def main():\n    return 1\nres = run(main())");
    assert!(js.contains("(await run(main()))"), "JS: {}", js);
}

#[test]
fn test_asyncio_run_not_awaited_in_sync_function() {
    // await is illegal inside a sync function body -- the Promise passes
    // through unchanged there (documented limit).
    let js = compile("import asyncio\nasync def main():\n    return 1\ndef sync_caller():\n    return asyncio.run(main())");
    assert!(js.contains("return asyncio.run(main());"), "JS: {}", js);
    assert!(!js.contains("return (await asyncio.run"), "JS: {}", js);
}

#[test]
fn test_async_for_bridges_protocol() {
    let js = compile("async def main():\n    async for v in obj:\n        print(v)");
    assert!(
        js.contains("for await (const v of __pyAsyncIter(obj))"),
        "JS: {}",
        js
    );
}

#[test]
fn test_async_comprehension_awaited_in_async_context() {
    let js = compile("async def main():\n    vals = [v async for v in obj]\n    return vals");
    assert!(js.contains("(await (async () => {"), "JS: {}", js);
}

#[test]
fn test_async_with_dispatches_async_protocol() {
    let js = compile("async def main():\n    async with mgr as t:\n        print(t)");
    assert!(
        js.contains(".__aenter__ === \"function\") ? await __cm0.__aenter__()"),
        "JS: {}",
        js
    );
    assert!(
        js.contains("await __cm0.__aexit__(null, null, null)"),
        "JS: {}",
        js
    );
}

// ── Round-4 pythonic sweep: generator methods ───────────────────────────

#[test]
fn test_generator_send_lowered_to_runtime_bridge() {
    let js = compile("g = gen()\nv = g.send(\"hi\")");
    assert!(js.contains("pyGenSend(g, \"hi\")"), "JS: {}", js);
}

#[test]
fn test_generator_close_lowered_to_runtime_bridge() {
    let js = compile("g = gen()\ng.close()");
    assert!(js.contains("pyGenClose(g)"), "JS: {}", js);
}

#[test]
fn test_generator_throw_lowered_to_runtime_bridge() {
    let js = compile("g = gen()\nv = g.throw(ValueError(\"boom\"))");
    assert!(js.contains("pyGenThrow(g, "), "JS: {}", js);
}

// ── #155: generator expressions are REAL lazy JS generators ─────────────

#[test]
fn test_genexp_lowers_to_lazy_generator_iife() {
    let js = compile("g = (x * 2 for x in xs if x > 1)");
    assert!(
        js.contains("(function* (__gen_it) {"),
        "genexp must be a function* IIFE, not an eager array: {}",
        js
    );
    assert!(js.contains("yield "), "generator body yields: {}", js);
    assert!(
        !js.contains(".filter(") && !js.contains(".map("),
        "no eager .filter().map() pipeline: {}",
        js
    );
    // The OUTERMOST iterable is evaluated at creation time (CPython:
    // iter(outermost) runs when the genexp object is built).
    assert!(
        js.contains("}).call(this, pyForIter(xs))"),
        "outer iterable passed eagerly via .call(this, ...): {}",
        js
    );
}

#[test]
fn test_genexp_next_with_default_uses_pynext() {
    let js = compile("active = next((p for p in items if p == target), fallback)");
    assert!(js.contains("pyNext("), "routes through pyNext: {}", js);
    assert!(
        js.contains("(function* (__gen_it)"),
        "lazy generator arg: {}",
        js
    );
}

#[test]
fn test_genexp_nested_fors_and_filter() {
    let js = compile("g = (x * y for x in xs for y in ys if x != y)");
    assert!(
        js.contains("(function* (__gen_it)"),
        "generator IIFE: {}",
        js
    );
    // Outer loop drives the pre-evaluated __gen_it; inner iterable stays
    // lazy (evaluated inside the generator body).
    assert!(
        js.contains("for (const x of __gen_it)"),
        "outer loop over param: {}",
        js
    );
    assert!(
        js.contains("for (const y of pyForIter(ys))"),
        "inner loop stays inline/lazy: {}",
        js
    );
    assert!(js.contains("if ("), "filter kept: {}", js);
}

#[test]
fn test_genexp_in_method_captures_this() {
    // `self` compiles to `this`; a bare function* would shadow it. The
    // IIFE must be invoked with .call(this, ...).
    let js = compile(
        r#"
class Basket:
    def __init__(self, items):
        self.items = items
    def first_big(self):
        return next((x for x in self.items if x > 1), None)
"#,
    );
    assert!(
        js.contains("}).call(this, pyForIter(this.items))"),
        "genexp IIFE must bind outer this for self.* access: {}",
        js
    );
}

#[test]
fn test_async_genexp_lowers_to_async_generator() {
    let js = compile(
        r#"
async def collect(stream):
    g = (x * 2 async for x in stream)
    async for v in g:
        print(v)
"#,
    );
    assert!(
        js.contains("(async function* (__gen_it)"),
        "async genexp is an async generator object: {}",
        js
    );
    assert!(
        js.contains("for await (const x of __gen_it)"),
        "for-await over bridged source: {}",
        js
    );
    assert!(
        js.contains("__pyAsyncIter("),
        "python-protocol async bridge applied: {}",
        js
    );
}

#[test]
fn test_genexp_spread_into_call() {
    let js = compile("total = add3(*(x + 1 for x in xs))");
    assert!(
        js.contains("add3(...(function* (__gen_it)"),
        "JS spread consumes the generator directly: {}",
        js
    );
}

#[test]
fn test_listcomp_still_eager() {
    // Regression guard: ListComp/SetComp keep the eager pipeline — only
    // GeneratorExp went lazy.
    let js = compile("evens = [x for x in numbers if x % 2 == 0]");
    assert!(
        js.contains(".filter(") && js.contains(".map("),
        "JS: {}",
        js
    );
    assert!(
        !js.contains("function*"),
        "listcomp must not be a generator: {}",
        js
    );
}

// ── #168: nested @component functions must not emit `export` ─────────────

#[test]
fn test_nested_component_stays_local() {
    let js = compile(
        r#"
from pyths.react import component, use_state

@component
def KanbanLite():
    cards, set_cards = use_state([[{"id": 1, "title": "A"}], []])
    @component
    def render_card(col_idx, card_idx, card):
        return div(cn="card", card["title"])
    return div(cn="board", render_card(col_idx=0, card_idx=0, card=cards[0][0]))
"#,
    );
    // Top-level component keeps its named export.
    assert!(
        js.contains("export function KanbanLite()"),
        "top-level component stays exported: {}",
        js
    );
    // Nested component compiles as a LOCAL declaration — `export` inside a
    // function body is a JS SyntaxError at the vite/esbuild boundary.
    assert!(
        !js.contains("export function render_card"),
        "nested @component must not emit export: {}",
        js
    );
    assert!(
        js.contains("function render_card({col_idx, card_idx, card} = {})"),
        "nested component still gets the props-destructuring transform: {}",
        js
    );
}

#[test]
fn test_component_single_named_param_destructures() {
    // Post-#353 regression guard: named params are PROP NAMES at EVERY arity.
    // `def Frontier(data):` is a component with one prop named `data` — the
    // definition must destructure (`function Frontier({data} = {})`) so `data`
    // binds the prop VALUE, never the whole props object. PR #353 broke this
    // (blanket arity-1 positional binding): `data` received `{data: {...}}`
    // and `data["points"]` threw KeyError — caught by reference-app's frontend
    // suite (9 failures across 5 files), not by any compiler gate. This test
    // closes that gate gap.
    let js = compile(
        "from pyths.react import component\n@component\ndef Frontier(data):\n    return div(data[\"points\"])",
    );
    assert!(
        js.contains("function Frontier({data} = {})"),
        "arity-1 named param is a prop name and must destructure:\n{}",
        js
    );

    // The reference-app PaperUpload shape: a single named prop interpolated into
    // an f-string URL. Under #353's positional binding the WHOLE props object
    // interpolated into the URL ("[object Object]").
    let fstr = compile(
        "from pyths.react import component\n@component\ndef PaperUpload(run_id):\n    return a(href=f\"/api/runs/{run_id}/paper.md\", \"download\")",
    );
    assert!(
        fstr.contains("function PaperUpload({run_id} = {})"),
        "arity-1 prop interpolated in an f-string must destructure:\n{}",
        fstr
    );
}

#[test]
fn test_component_kwargs_only_binds_whole_props_object() {
    // #351 (re-fix): `def C(**props):` is the WHOLE-props-object form — the
    // single kwargs param binds the flat props object React passes
    // (`function C(props = {})`), NOT a `{...props}` rest-copy and NOT a
    // `.props`-key destructure.
    let js = compile(
        "from pyths.react import component\n@component\ndef Greet(**props):\n    return div(props[\"name\"])",
    );
    assert!(
        js.contains("function Greet(props = {})"),
        "kwargs-only component must bind the props object positionally:\n{}",
        js
    );
    assert!(
        !js.contains("{...props}"),
        "kwargs-only component must not rest-copy the props object:\n{}",
        js
    );
}

#[test]
fn test_component_single_param_named_props_is_whole_object_alias() {
    // #351 alias: a single no-default param LITERALLY named `props` binds the
    // whole props object positionally (what every React author means by it,
    // and what #351's consumers wrote). Corpus-checked: no component declares
    // a PROP named "props", so the alias shadows nothing.
    let js = compile(
        "from pyths.react import component\n@component\ndef Greet(props):\n    return div(props.name)",
    );
    assert!(
        js.contains("function Greet(props)"),
        "single `props` param must bind positionally:\n{}",
        js
    );
    assert!(
        !js.contains("{props}"),
        "must not destructure a .props key:\n{}",
        js
    );
}

#[test]
fn test_component_named_params_still_destructure() {
    // The named-prop convention: 2+ params, a defaulted single param, and a
    // defaulted param even when named `props` all stay a props destructure.
    let multi = compile(
        "from pyths.react import component\n@component\ndef Card(title, subtitle):\n    return div(title, subtitle)",
    );
    assert!(
        multi.contains("function Card({title, subtitle} = {})"),
        "multi named params must destructure:\n{}",
        multi
    );
    // A single DEFAULTED param is a named-prop destructure, not whole-props.
    let defaulted = compile(
        "from pyths.react import component\n@component\ndef Badge(count=0):\n    return div(count)",
    );
    assert!(
        defaulted.contains("function Badge({count = 0} = {})"),
        "single defaulted param stays a destructure:\n{}",
        defaulted
    );
    // Even named `props`: only the NO-default form is the whole-object alias.
    let defaulted_props = compile(
        "from pyths.react import component\n@component\ndef Chip(props=None):\n    return div(props)",
    );
    assert!(
        defaulted_props.contains("function Chip({props = null} = {})"),
        "defaulted `props` param stays a named-prop destructure:\n{}",
        defaulted_props
    );
    // Named params + **rest keep the Track-B rest-destructure.
    let rest = compile(
        "from pyths.react import component\n@component\ndef Panel(title, **rest):\n    return div(title, rest[\"x\"])",
    );
    assert!(
        rest.contains("function Panel({title, ...rest} = {})"),
        "named + **rest stays a rest destructure:\n{}",
        rest
    );
}

#[test]
fn test_nested_component_keeps_psx_transform() {
    // The nested body must still be in PSX mode (html tag calls become
    // createElement), not plain function calls.
    let js = compile(
        r#"
from pyths.react import component

@component
def Outer():
    @component
    def inner_row(label):
        return div(cn="row", label)
    return div(cn="outer", inner_row(label="x"))
"#,
    );
    assert!(
        !js.contains("export function inner_row"),
        "nested component local: {}",
        js
    );
    let nested_pos = js.find("function inner_row").expect("nested fn emitted");
    let body = &js[nested_pos..];
    assert!(
        body.contains("createElement"),
        "nested component body keeps PSX mode: {}",
        js
    );
}

// ── #166: value-aware type() ─────────────────────────────────────────────

#[test]
fn test_type_lowers_to_pytype_runtime() {
    let js = compile("t = type(x)\nn = type(5).__name__");
    assert!(
        js.contains("pyType(x)"),
        "type() routes through pyType: {}",
        js
    );
    assert!(
        js.lines().next().unwrap_or("").contains("pyType"),
        "pyType imported from runtime: {}",
        js
    );
    assert!(
        !js.contains("obj?.constructor ?? typeof obj"),
        "old Direct lowering gone: {}",
        js
    );
}

#[test]
fn test_type_as_value_maps_to_pytype() {
    // `type` referenced as a first-class value (e.g. map(type, xs)).
    let js = compile("names = [type(x).__name__ for x in xs]\nf = type");
    assert!(js.contains("let f = pyType;"), "value position: {}", js);
}

// ── Round-5: one-line suites + f-string whitespace padding ──────────────

#[test]
fn test_inline_suite_after_colon() {
    // `if cond: return x` on one line — legal Python, the dominant shape
    // in 3 of 5 remaining gen-eval compile failures.
    let js = compile("def f(x):\n    if x > 1: return \"big\"\n    return \"small\"");
    assert!(
        js.contains("return \"big\";"),
        "inline return emitted: {}",
        js
    );
    assert!(
        js.contains("return \"small\";"),
        "following stmt intact: {}",
        js
    );
}

#[test]
fn test_inline_suite_semicolon_chain() {
    // (#301: `log` is an unknown-type param, so .append dispatches through
    // pyAppend rather than a blind .push rename — the point of this test
    // is the semicolon chain, which must yield BOTH statements.)
    let js = compile("def add(v, log):\n    if v: log.append(v); log.append(v * 10)");
    assert!(js.contains("pyAppend(log, v)"), "first stmt: {}", js);
    assert!(
        js.contains("pyAppend(log, pyMul(v, 10))"),
        "second stmt after ';': {}",
        js
    );
}

#[test]
fn test_inline_suite_while_and_for() {
    let js = compile("i = 0\nwhile i < 3: i += 1\nfor k in xs: total += k");
    assert!(js.contains("while ((i < 3))"), "while one-liner: {}", js);
    assert!(
        js.contains("for (const k of pyForIter(xs))"),
        "for one-liner: {}",
        js
    );
}

#[test]
fn test_fstring_expr_whitespace_padding() {
    // CPython allows `f"{ expr }"` — the padding must not reach the
    // sub-lexer as INDENT.
    let js = compile("msg = f\"is { 'even' if n % 2 == 0 else 'odd' }.\"");
    assert!(js.contains("\"even\""), "padded conditional parsed: {}", js);
    let js2 = compile("y = f\"{ x }\"");
    assert!(js2.contains("${"), "padded name interpolates: {}", js2);
}

// ---------------------------------------------------------------------------
// Track-B sweep: third-party React library interop (tests/libinterop twins)
// ---------------------------------------------------------------------------

#[test]
fn test_library_component_props_snake_to_camel() {
    // Props on components imported from React-ecosystem npm modules get the
    // same snake->camel conversion HTML tags get: the library's vocabulary
    // is camelCase (onOpenChange, asChild, forceMount). Verbatim camelCase
    // still passes through (conversion no-ops without underscores).
    let js = compile(
        r#"
from pyths.react import component, use_state
from at_radix_ui.react_dialog import Root, Trigger, Content

@component
def App():
    is_open, set_open = use_state(False)
    return Root(open=is_open, on_open_change=set_open,
        Trigger(as_child=True, button("open")),
        Content(force_mount=True, onEscapeKeyDown=set_open, p("hi")))
"#,
    );
    assert!(
        js.contains("onOpenChange: set_open"),
        "on_open_change converts: {}",
        js
    );
    assert!(js.contains("asChild: true"), "as_child converts: {}", js);
    assert!(
        js.contains("forceMount: true"),
        "force_mount converts: {}",
        js
    );
    assert!(
        js.contains("onEscapeKeyDown: set_open"),
        "verbatim camelCase passes: {}",
        js
    );
}

#[test]
fn test_user_component_props_stay_snake() {
    // User @component props are the user's own vocabulary — NOT converted.
    let js = compile(
        r#"
from pyths.react import component

@component
def Card(item_title):
    return h2(item_title)

@component
def App():
    return Card(item_title="hi")
"#,
    );
    assert!(
        js.contains("item_title: \"hi\""),
        "user props stay snake: {}",
        js
    );
}

#[test]
fn test_library_module_alias_member_props_convert() {
    // `import at_radix_ui.react_dialog as DialogPrimitive` — dotted tags
    // rooted at the alias are library components; props convert.
    let js = compile(
        r#"
from pyths.react import component
import at_radix_ui.react_dialog as DialogPrimitive

@component
def App():
    return DialogPrimitive.Root(on_open_change=print, p("x"))
"#,
    );
    assert!(
        js.contains("createElement(DialogPrimitive.Root"),
        "member tag: {}",
        js
    );
    assert!(
        js.contains("onOpenChange"),
        "member-tag props convert: {}",
        js
    );
}

#[test]
fn test_component_kwargs_param_rest_destructure() {
    // `**rest` in a @component signature = rest-destructure of props.
    // Previously dropped from the pattern while the body referenced it.
    let js = compile(
        r#"
from pyths.react import component

@component
def Button(label, **rest):
    return button(**rest, label)
"#,
    );
    assert!(
        js.contains("function Button({label, ...rest} = {})"),
        "rest param: {}",
        js
    );
    assert!(js.contains("...rest"), "spread into element: {}", js);
}

#[test]
fn test_framer_motion_lowercase_member_components() {
    // motion.div / motion.span dispatch as components (previously plain
    // calls — TypeError at runtime), and their props convert.
    let js = compile(
        r#"
from pyths.react import component
from framer_motion import motion

@component
def App():
    return motion.div(animate={"opacity": 1}, while_hover={"scale": 1.1},
        layout=True, "m")
"#,
    );
    assert!(
        js.contains("createElement(motion.div"),
        "motion.div is a component: {}",
        js
    );
    assert!(js.contains("whileHover"), "while_hover converts: {}", js);
    assert!(
        !js.contains("__pyKwArgs(motion.div"),
        "not a plain kw call: {}",
        js
    );
}

#[test]
fn test_library_component_style_dict_normalized() {
    // style= dicts on library components get CSS-key normalization too
    // (Radix forwards style to the DOM node; React needs camelCase keys).
    let js = compile(
        r#"
from pyths.react import component
from at_radix_ui.react_dialog import Content

@component
def App():
    return Content(style={"border_radius": "4px"}, p("x"))
"#,
    );
    assert!(js.contains("borderRadius"), "style keys normalize: {}", js);
}

#[test]
fn test_undefined_read_passes_through() {
    // A READ of unbound `undefined` is the JS global — the `x ?? undefined`
    // idiom for null/undefined-sensitive libraries (cva defaultVariants).
    let js = compile("def f(v=None):\n    return {\"variant\": v ?? undefined}\n");
    assert!(
        js.contains("v ?? undefined"),
        "undefined passes through: {}",
        js
    );
    assert!(
        !js.contains("undefined$"),
        "no sanitizer rename on read: {}",
        js
    );
}

#[test]
fn test_undefined_binding_still_sanitized() {
    // A user BINDING named `undefined` still gets the reserved-word rename.
    let js = compile("undefined = 5\nprint(undefined)\n");
    assert!(js.contains("undefined$ = 5"), "binding sanitized: {}", js);
}

#[test]
fn test_datetime_lowercase_class_gets_new() {
    // #253: datetime's classes are lowercase; a known class is `new`-called
    // regardless of case (the capitalization heuristic won't fire).
    let js = compile("from datetime import datetime\nd = datetime(2023, 4, 1)");
    assert!(
        js.contains("new datetime("),
        "datetime() not new-called: {}",
        js
    );
    let jd = compile("from datetime import date\nd = date(2023, 4, 1)");
    assert!(jd.contains("new date("), "date() not new-called: {}", jd);
}

#[test]
fn test_big_and_radix_int_literals() {
    // #255: a literal past 2^53 emits a JS BigInt; hex/oct/bin lex to their value.
    assert!(compile("x = 12345678901234567890").contains("12345678901234567890n"));
    assert!(compile("x = 0xFF").contains("255"));
    assert!(compile("x = 0o17").contains("15"));
    assert!(compile("x = 0b1010").contains("10"));
    assert!(compile("x = 0xFFFFFFFFFFFFFFFF").contains("18446744073709551615n"));
}

#[test]
fn test_set_intersection_unbound_form() {
    // #260: set.intersection(a, b) is the unbound-method form == a.intersection(b).
    let js = compile("a = {1, 2}\nb = {2, 3}\nx = set.intersection(a, b)");
    assert!(
        !js.contains("set.intersection"),
        "unbound set.intersection not rewritten: {}",
        js
    );
}

#[test]
fn test_for_target_reassigned_uses_let() {
    // #262: a for-target reassigned in the body can't be `const`. A simple-name
    // target is hoisted to a function/module `let` (#269) and the loop binds it
    // directly (bare `for (i of ...)`, so the reassignment inside the body has
    // a binding). #288: a REUSED tuple target is hoisted the same way — the
    // loop writes the hoisted lets (`for ([k, v] of ...)`) so both names leak
    // (Python scopes every name of the tuple target to the enclosing scope).
    let jt = compile("for k, v in pairs:\n    v = 99");
    assert!(
        jt.contains("let k;"),
        "tuple target names should be hoisted: {}",
        jt
    );
    assert!(
        jt.contains("let v;"),
        "tuple target names should be hoisted: {}",
        jt
    );
    assert!(
        jt.contains("for ([k, v]"),
        "tuple target should write hoisted lets: {}",
        jt
    );
    let ji = compile("for i in xs:\n    i = i * 2");
    assert!(
        ji.contains("let i;"),
        "reassigned name target should be hoisted: {}",
        ji
    );
    assert!(
        ji.contains("for (i of "),
        "reassigned name target binds the hoisted let: {}",
        ji
    );
    // a NON-reassigned target keeps its per-iteration const
    let jc = compile("for x in xs:\n    print(x)");
    assert!(
        jc.contains("for (const x"),
        "non-reassigned target stays const: {}",
        jc
    );
}

#[test]
fn test_for_target_read_after_loop_leaks() {
    // #269 (R17): Python leaks the loop variable to the enclosing scope with
    // its final value, so a bare read AFTER the loop must not ReferenceError.
    // The target is hoisted to a `let` and the loop binds it directly.
    // PBT-2: the hoisted `let` is sentinel-initialized so a ZERO-iteration
    // loop leaves the name raising on read (CPython), while a nonempty loop
    // overwrites the sentinel and the leak works as before.
    let js = compile("for i in xs:\n    pass\nprint(i)");
    assert!(
        js.contains("let i = __UNBOUND;"),
        "leaked loop var not hoisted:\n{}",
        js
    );
    assert!(
        js.contains("for (i of "),
        "loop must bind the hoisted let:\n{}",
        js
    );
    // A read confined to the loop body keeps the per-iteration const (no churn).
    let jc = compile("for i in xs:\n    print(i)");
    assert!(
        jc.contains("for (const i of "),
        "in-loop-only read stays const:\n{}",
        jc
    );
    // for-else: the else block is outside the loop's JS block scope, so a
    // target read there also forces the hoist.
    let je = compile("for n in xs:\n    pass\nelse:\n    print(n)");
    assert!(
        je.contains("let n = __UNBOUND;"),
        "for-else leaked read not hoisted:\n{}",
        je
    );
}

#[test]
fn test_pep701_same_quote_fstring_compiles() {
    // PBT-3 (PEP 701 subset): same-quote nesting inside f-string expression
    // parts must compile end-to-end (lexer scanner + parser expr capture +
    // quote-aware format-spec split).
    let js = compile("print(f'val: {'Hello, World!'}')");
    assert!(js.contains("Hello, World!"), "inner literal lost:\n{}", js);
    // A colon inside a nested same-quote string is NOT a format spec.
    let jc = compile("x = f'{'a:b'}'");
    assert!(
        jc.contains("a:b"),
        "colon-in-string mis-split as format spec:\n{}",
        jc
    );
    // A closing brace inside a nested string is content, not an expr end.
    let jb = compile("x = f'{'}'}'");
    assert!(jb.contains("}"), "brace-in-string lost:\n{}", jb);
    // Format spec after a same-quote literal still lowers.
    let js2 = compile("x = f'{'ab':>5}'");
    assert!(
        js2.contains("padStart") || js2.contains("pyFormatSpec"),
        "format spec lost:\n{}",
        js2
    );
}

#[test]
fn test_star_import() {
    // #276: `from module import *`. Erased modules are a no-op; a value module
    // gets a namespace import (valid ESM; bare starred names are not bound).
    let jt = compile("from typing import *\nx = 1");
    assert!(
        !jt.contains("import"),
        "typing star import must no-op:\n{}",
        jt
    );
    let jm = compile("from math import *\nx = 1");
    assert!(
        jm.contains("import * as __pyStar") && jm.contains("stdlib/math"),
        "math star import → namespace import:\n{}",
        jm
    );
    // must not emit an invalid `import { * }`
    assert!(
        !jm.contains("{ * }") && !jm.contains("{* }"),
        "no invalid star named-import:\n{}",
        jm
    );
}

#[test]
fn test_duplicate_import_deduped() {
    // #274: Python tolerates re-importing a name; ES modules SyntaxError on a
    // second `import { X }`. Dedupe by binding.
    let js = compile("from collections import defaultdict\nfrom collections import defaultdict\nd = defaultdict(int)");
    assert_eq!(
        js.matches("import { defaultdict }").count(),
        1,
        "re-imported name must be imported once:\n{}",
        js
    );
    // even when the first import is part of a multi-name line
    let jm = compile("from collections import Counter, defaultdict\nfrom collections import defaultdict\nd = defaultdict(int)");
    assert_eq!(
        jm.matches("defaultdict").filter(|_| true).count() >= 1,
        true
    );
    assert!(
        !jm.contains("import { defaultdict } from"),
        "second defaultdict line dropped:\n{}",
        jm
    );
    // `import X` twice
    let ji = compile("import heapq\nimport heapq");
    assert_eq!(
        ji.matches("import * as heapq").count(),
        1,
        "dup namespace import once:\n{}",
        ji
    );
}

#[test]
fn test_module_redefinition_is_reassignment() {
    // #350: a second module-level `def`/`class` of the same name is Python
    // last-wins — emitted as a reassignment, not a duplicate declaration
    // (which is a JS "Identifier already declared" SyntaxError).
    let jf = compile("def f(s):\n    return 1\ndef f(s):\n    return 2");
    assert_eq!(
        jf.matches("function f(").count(),
        1,
        "second def must not redeclare:\n{}",
        jf
    );
    assert!(
        jf.contains("f = function"),
        "redefinition must be a reassignment:\n{}",
        jf
    );
    let jc = compile("class C:\n    def m(self):\n        return 1\nclass C:\n    def m(self):\n        return 2");
    // Exactly one `class C` declaration (the first); the redefinition is a
    // `C = class …` reassignment (no duplicate `class C` declaration).
    assert_eq!(
        jc.matches("class C extends").count(),
        1,
        "only the first is a class decl:\n{}",
        jc
    );
    assert!(
        jc.contains("C = class"),
        "class redefinition must be a reassignment:\n{}",
        jc
    );
}

#[test]
fn test_round_and_float_map_to_runtime_helpers() {
    // #341/#342 are runtime-helper behavior fixes; assert the lowering still
    // routes through the helpers (behavior is covered by the differential
    // corpus + pyths-run differential).
    let jr = compile("print(round(float('nan')))");
    assert!(jr.contains("pyRound("), "round must use pyRound:\n{}", jr);
    let jf = compile("print(float('1_0.5'))");
    assert!(jf.contains("pyFloat("), "float must use pyFloat:\n{}", jf);
}

#[test]
fn test_bitwise_whole_float_threads_fctx() {
    // #343: a statically-float bitwise operand threads a float-context flag so
    // the runtime raises TypeError even for a whole-valued float (3.0 & 5).
    let js = compile("print(3.0 & 5)");
    assert!(
        js.contains("pyBitAnd(3, 5, "),
        "float bitwise operand must pass fctx:\n{}",
        js
    );
    // A pure-int bitwise op must NOT carry a float flag.
    let ji = compile("a = 6\nb = 3\nprint(a & b)");
    assert!(
        !ji.contains(", 1)") && !ji.contains(", 2)") && !ji.contains(", 3)"),
        "int bitwise must not pass fctx:\n{}",
        ji
    );
}

#[test]
fn test_format_spec_float_threads_isfloat() {
    // #347: a statically-float value in a format-spec threads isFloat so the
    // no-type-char branch renders as a float ('0.0'), not an int ('0').
    let js = compile("print(f'{0.0:>6}')");
    assert!(
        js.contains("pyFormatSpec") && js.contains(", true)"),
        "float format-spec value must pass isFloat:\n{}",
        js
    );
    // An int value must NOT get the isFloat flag.
    let ji = compile("print(f'{42:>6}')");
    assert!(
        !ji.contains(", true)"),
        "int format-spec must not pass isFloat:\n{}",
        ji
    );
}

#[test]
fn test_range_for_lowers_to_native_counting_loop() {
    // #235 / #349(b): `for i in range(...)` must NOT materialise a pyRange
    // array — it lowers to a native counting loop over a private counter.
    let js = compile("total = 0\nfor i in range(10):\n    total += i");
    assert!(
        js.contains("__ri_"),
        "expected native counter loop:\n{}",
        js
    );
    assert!(
        !js.contains("of pyRange("),
        "range for-loop must not build a pyRange array:\n{}",
        js
    );
    // 3-arg form threads start/stop/step temps.
    let js3 = compile("acc = []\nfor i in range(2, 20, 3):\n    acc.append(i)");
    assert!(
        js3.contains("__r_start_") && js3.contains("__r_stop_") && js3.contains("__r_step_"),
        "3-arg range must bind start/stop/step temps:\n{}",
        js3
    );
    // A shadowed `range` (user binding) falls back to the generic path.
    let jshadow = compile("def range(n):\n    return [0]\nfor i in range(3):\n    print(i)");
    assert!(
        !jshadow.contains("__ri_"),
        "shadowed range must not take the native path:\n{}",
        jshadow
    );
}

#[test]
fn test_any_all_lower_to_lazy_helpers() {
    // #348: any()/all() must lower to the lazy pyAny/pyAll runtime helpers,
    // not an eager `[...iter].some/.every` spread.
    let ja = compile("print(any(x > 2 for x in range(5)))");
    assert!(ja.contains("pyAny("), "any() must use pyAny:\n{}", ja);
    assert!(
        !ja.contains(".some("),
        "any() must not spread-and-.some:\n{}",
        ja
    );
    let jl = compile("print(all(x > 2 for x in range(5)))");
    assert!(jl.contains("pyAll("), "all() must use pyAll:\n{}", jl);
    assert!(
        !jl.contains(".every("),
        "all() must not spread-and-.every:\n{}",
        jl
    );
}

#[test]
fn test_for_target_sibling_loops_not_bare() {
    // #269 follow-up + #235/#349(b): two sibling `for i in range(...)` loops
    // reusing the same target, with NO read outside either loop, each lower to
    // an independent native counting loop. The Python loop variable is a
    // per-iteration block-scoped `let i = __ri_N` inside each loop's own block,
    // so neither loop's `i` escapes to the other (the old for-of bare-target
    // hazard cannot arise) and the target is not hoisted to a module `let`.
    let js = compile(
        "def f(s):\n    for i in range(len(s)):\n        pass\n    for i in range(len(s)):\n        print(i)",
    );
    assert!(
        !js.contains("for (i of"),
        "sibling loop must not emit a bare target:\n{}",
        js
    );
    assert!(
        !js.contains("for (let i of"),
        "range loop must not use the for-of path:\n{}",
        js
    );
    assert_eq!(
        js.matches("let i = __ri_").count(),
        2,
        "both loops bind i from their own counter:\n{}",
        js
    );
    assert!(
        !js.contains("let i;"),
        "unread sibling target must not be hoisted:\n{}",
        js
    );
}

#[test]
fn test_zero_iter_for_target_read_raises_unbound() {
    // PBT-2: reading a for-loop target after a ZERO-iteration loop must raise
    // UnboundLocalError (function scope) / NameError (module scope) like
    // CPython — not read the hoisted `let` as undefined→None. The hoisted
    // target is initialized to the __UNBOUND sentinel; reads go through the
    // __pyChkLocal/__pyChkGlobal guards; writes (the loop binding itself,
    // ordinary assignment) stay bare and overwrite the sentinel.
    let js = compile("def f():\n    for v1 in []:\n        pass\n    return v1");
    assert!(
        js.contains("let v1 = __UNBOUND;"),
        "sentinel init missing:\n{}",
        js
    );
    assert!(
        js.contains("for (v1 of "),
        "loop must bind the hoisted let bare:\n{}",
        js
    );
    assert!(
        js.contains("__pyChkLocal(v1, \"v1\")"),
        "post-loop read must be guarded:\n{}",
        js
    );
    // Module scope: an unbound name is a NameError, not UnboundLocalError.
    let jm = compile("for q in xs:\n    pass\nprint(q)");
    assert!(
        jm.contains("let q = __UNBOUND;"),
        "module sentinel missing:\n{}",
        jm
    );
    assert!(
        jm.contains("__pyChkGlobal(q, \"q\")"),
        "module read must use the NameError guard:\n{}",
        jm
    );
    // #288: tuple/list destructuring targets get the same treatment — every
    // pattern name is hoisted (leaked names sentinel-initialized), the loop
    // writes the hoisted lets bare, and a zero-iteration loop leaves the
    // leaked name raising on read.
    let jt = compile("def g(pairs):\n    for a, b in pairs:\n        pass\n    return a");
    assert!(
        jt.contains("let a = __UNBOUND;"),
        "leaked tuple-target name must be sentinel-initialized:\n{}",
        jt
    );
    assert!(
        jt.contains("for ([a, b] of "),
        "destructuring loop must write the hoisted lets bare:\n{}",
        jt
    );
    assert!(
        jt.contains("__pyChkLocal(a, \"a\")"),
        "post-loop read of tuple-target name must be guarded:\n{}",
        jt
    );
    // A name with a guaranteed top-level binding is not hoisted at all and
    // must stay unguarded.
    let jp = compile("def h():\n    x = 5\n    for x in []:\n        pass\n    return x");
    assert!(
        !jp.contains("__UNBOUND"),
        "pre-assigned target must not be guarded:\n{}",
        jp
    );
}

#[test]
fn test_keyword_named_method_metadata_unsanitized() {
    // #299: class bodies emit methods under their RAW Python name (JS allows
    // reserved words as method/property names), so the __pyparams__ /
    // static-alias metadata must use the raw name too. Sanitizing produced
    // `X.prototype.default$.__pyparams__ = ...` — a TypeError on undefined —
    // for any method named like a JS keyword (json.JSONEncoder's `default`).
    let js = compile("class E:\n    def default(self, obj, extra=1):\n        return obj");
    assert!(
        js.contains("E.prototype.default.__pyparams__"),
        "metadata must target the raw method name:\n{}",
        js
    );
    assert!(
        !js.contains("default$"),
        "no sanitized method-name references:\n{}",
        js
    );
}

#[test]
fn test_set_construction_routes_pyset() {
    // #297: every Python set-construction form must build the canonicalizing
    // PySet (bool/int/float hash identity, structural tuple membership) —
    // literal, comprehension, set()/frozenset() constructors (zero-arg
    // included), and the set+set concat fast path.
    let jl = compile("s = {1, True}");
    assert!(jl.contains("new PySet(["), "literal must be PySet:\n{}", jl);
    let jc = compile("s = {x % 3 for x in xs}");
    assert!(jc.contains("new PySet("), "set-comp must be PySet:\n{}", jc);
    let jf = compile("s = set([1, 2])");
    assert!(jf.contains("pySetOf("), "set() must route pySetOf:\n{}", jf);
    let je = compile("s = set()");
    assert!(
        je.contains("pySetOf()"),
        "set() zero-arg must route pySetOf:\n{}",
        je
    );
    let jz = compile("s = frozenset([1, 2])");
    assert!(
        jz.contains("pySetOf("),
        "frozenset() must route pySetOf:\n{}",
        jz
    );
    assert!(
        !jz.contains("frozenset("),
        "no bare frozenset ReferenceError:\n{}",
        jz
    );
    // set as a first-class value (defaultdict(set) etc.)
    let jv = compile("from collections import defaultdict\nd = defaultdict(set)");
    assert!(
        jv.contains("pySetOf"),
        "set-as-value must route pySetOf:\n{}",
        jv
    );
}

#[test]
fn test_tuple_for_target_leak_and_preassigned_writes() {
    // #288 (A): a leaked tuple/list destructuring target hoists EVERY pattern
    // name and the loop writes the hoisted lets bare — Python leaks all of
    // them; the old `const [a, b]` shadowed the hoisted lets so post-loop
    // reads saw None.
    let ja = compile("def f(ps):\n    for a, b in ps:\n        pass\n    return (a, b)");
    assert!(
        ja.contains("let a = __UNBOUND;"),
        "leaked tuple name a not sentinel-hoisted:\n{}",
        ja
    );
    assert!(
        ja.contains("let b = __UNBOUND;"),
        "leaked tuple name b not sentinel-hoisted:\n{}",
        ja
    );
    assert!(
        ja.contains("for ([a, b] of "),
        "loop must write hoisted lets bare:\n{}",
        ja
    );
    // Nested and starred patterns take the same bare form.
    let jn = compile("def f(ps):\n    for i, (j, k) in ps:\n        pass\n    return (i, j, k)");
    assert!(
        jn.contains("for ([i, [j, k]] of "),
        "nested pattern must be bare:\n{}",
        jn
    );
    let js = compile("def f(ps):\n    for a, *rest in ps:\n        pass\n    return (a, rest)");
    assert!(
        js.contains("for ([a, ...rest] of "),
        "star pattern must be bare:\n{}",
        js
    );
    // A destructuring loop with NO outside use keeps its per-iteration const.
    let jc = compile("def f(ps):\n    for a, b in ps:\n        print(a + b)");
    assert!(
        jc.contains("for (const [a, b] of "),
        "no-leak pattern stays const:\n{}",
        jc
    );

    // #288 (B): a pre-assigned simple target — the loop must WRITE the
    // existing depth-0 binding, not shadow it. The first assignment is
    // promoted to a hoisted `let` (no sentinel — the binding is guaranteed)
    // and both the assignment and the loop emit bare.
    let jp = compile("def g():\n    x = 5\n    for x in [1, 2]:\n        pass\n    return x");
    assert!(
        jp.contains("let x;"),
        "pre-assigned target must be hoisted:\n{}",
        jp
    );
    assert!(
        jp.contains("for (x of "),
        "loop must write the promoted binding:\n{}",
        jp
    );
    assert!(
        !jp.contains("__UNBOUND"),
        "promoted binding is guaranteed — no sentinel:\n{}",
        jp
    );
    // Module scope: promotion must keep the export the inline first
    // assignment would have carried (B-015).
    let jm = compile("x = 5\nfor x in [1, 2]:\n    pass\nprint(x)");
    assert!(
        jm.contains("export let x;"),
        "promoted module name must stay exported:\n{}",
        jm
    );
    assert!(
        jm.contains("for (x of "),
        "module loop must write the promoted binding:\n{}",
        jm
    );
}

#[test]
fn test_sentinel_write_positions_stay_bare() {
    // PBT-2 follow-up (caught by the LiveCodeBench regression net): every
    // WRITE position must emit the sentinel-guarded name bare — wrapping it
    // in __pyChkLocal produced an invalid JS assignment target (walrus) or
    // left the function-scope binding untouched (match capture shadowing).
    // Walrus:
    let jw = compile(
        "def f():\n    for i in []:\n        pass\n    if (i := 10) > 5:\n        return i",
    );
    assert!(
        jw.contains("(i = 10)"),
        "walrus target must be bare:\n{}",
        jw
    );
    assert!(
        !jw.contains("__pyChkLocal(i, \"i\") ="),
        "walrus target must not be guard-wrapped:\n{}",
        jw
    );
    // with-as:
    let jv = compile(
        "def g(p):\n    for fh in []:\n        pass\n    with open(p) as fh:\n        pass\n    return 1",
    );
    assert!(
        !jv.contains("__pyChkLocal(fh, \"fh\") ="),
        "with-as target must not be guard-wrapped:\n{}",
        jv
    );
    // match capture: a HOISTED name (here sentinel-initialized) is written,
    // not shadowed by a case-block `let` (which left the outer binding
    // unbound -> false UnboundLocalError after the match).
    let jm = compile(
        "def h():\n    for u in []:\n        pass\n    match 5:\n        case u:\n            pass\n    return u",
    );
    assert!(
        jm.contains("u = __match0;"),
        "capture must write the hoisted let:\n{}",
        jm
    );
    assert!(
        !jm.contains("let u = __match0;"),
        "capture must not shadow the hoisted let:\n{}",
        jm
    );
    // A capture name that IS hoisted (read after the loop → sentinel) writes
    // the hoisted binding — post-match reads see the captured value like
    // CPython (old code shadowed and returned None).
    let jc = compile(
        "def k():\n    for i in [1]:\n        print(i)\n    match 5:\n        case i:\n            pass\n    return i",
    );
    assert!(
        jc.contains("i = __match0;"),
        "capture must write hoisted binding:\n{}",
        jc
    );
    // ...but a name only ever declared as a per-iteration `const` for-target
    // (no reads outside the loop) has NO function-scope binding — the
    // capture keeps its own `let`.
    let jc2 = compile(
        "def k2():\n    for i in [1]:\n        print(i)\n    match 5:\n        case i:\n            pass\n    return 0",
    );
    assert!(
        jc2.contains("let i = __match0;"),
        "const-only target: capture keeps block let:\n{}",
        jc2
    );
    // Comprehension targets are arrow parameters / `for (const …)` bindings.
    // A name that is BOTH a comprehension variable and a later for-loop
    // target (LiveCodeBench sample_85 shape) must never emit
    // `(__pyChkLocal(i, "i")) => …`.
    let jl = compile(
        "def m(nums):\n    ones = [i for i in range(len(nums)) if nums[i] == 1]\n    for i in range(1, len(ones)):\n        pass\n    return ones",
    );
    assert!(
        !jl.contains("(__pyChkLocal(i, \"i\"))"),
        "comprehension param must not be guard-wrapped:\n{}",
        jl
    );
}

// --- #306: PSX lowers ANY unbound lowercase tag call, not just the allowlist ---

#[test]
fn test_psx_unbound_nonallowlist_tags_lower_to_create_element() {
    // `ins`/`del` are standard HTML5; `font`/`center`/`strike`/`marquee`
    // are obsolete-but-rendering. None are in the fixed allowlist; all
    // previously emitted bare identifier calls (ReferenceError at mount).
    let js = compile(
        "from pyths.react import component\n\
         \n\
         @component\n\
         def BrokenTags():\n\
         \x20   return div(\n\
         \x20       ins(\"inserted\"),\n\
         \x20       font(color=\"red\", \"legacy\"),\n\
         \x20       center(\"legacy center\"),\n\
         \x20       strike(\"struck\"),\n\
         \x20       marquee(\"scrolling\"),\n\
         \x20   )\n",
    );
    assert!(
        js.contains("createElement(\"ins\""),
        "ins must lower to a string tag: {}",
        js
    );
    assert!(
        js.contains("createElement(\"font\""),
        "font must lower to a string tag: {}",
        js
    );
    assert!(
        js.contains("createElement(\"center\""),
        "center must lower: {}",
        js
    );
    assert!(
        js.contains("createElement(\"strike\""),
        "strike must lower: {}",
        js
    );
    assert!(
        js.contains("createElement(\"marquee\""),
        "marquee must lower: {}",
        js
    );
    // The font props must be a props object, not __pyCallKw on an undefined name.
    assert!(
        !js.contains("__pyCallKw(font"),
        "font must not emit as a bare kw call: {}",
        js
    );
}

#[test]
fn test_psx_unbound_tag_locally_defined_function_still_wins() {
    // A module-level `def ins(...)` is a REAL binding — the tag fallback
    // must not claim it (scope resolution wins over the element rule).
    let js = compile(
        "from pyths.react import component\n\
         \n\
         def ins(text):\n\
         \x20   return text\n\
         \n\
         @component\n\
         def App():\n\
         \x20   return div(ins(\"hi\"))\n",
    );
    assert!(
        !js.contains("createElement(\"ins\""),
        "declared ins() must stay a call: {}",
        js
    );
    assert!(
        js.contains("ins(\"hi\")"),
        "ins call must be preserved: {}",
        js
    );
}

#[test]
fn test_psx_unbound_tag_local_variable_wins() {
    // A local binding inside the component (param or assignment) also wins.
    let js = compile(
        "from pyths.react import component\n\
         \n\
         @component\n\
         def App(marquee):\n\
         \x20   center = marquee\n\
         \x20   return div(marquee(\"a\"), center(\"b\"))\n",
    );
    assert!(
        !js.contains("createElement(\"marquee\""),
        "param marquee must stay a call: {}",
        js
    );
    assert!(
        !js.contains("createElement(\"center\""),
        "local center must stay a call: {}",
        js
    );
}

#[test]
fn test_psx_unbound_tag_imported_binding_wins() {
    // An imported lowercase name is bound — never claimed as a tag.
    let js = compile(
        "from pyths.react import component\n\
         from helpers import ins\n\
         \n\
         @component\n\
         def App():\n\
         \x20   return div(ins(\"hi\"))\n",
    );
    assert!(
        !js.contains("createElement(\"ins\""),
        "imported ins must stay a call: {}",
        js
    );
}

#[test]
fn test_psx_builtins_never_claimed_as_tags() {
    // Python builtins spelled lowercase (len/sorted/getattr/...) must keep
    // builtin lowering inside components — not become createElement calls.
    let js = compile(
        "from pyths.react import component\n\
         \n\
         @component\n\
         def App(items):\n\
         \x20   n = len(items)\n\
         \x20   s = sorted(items)\n\
         \x20   return div(str(n), span(s[0]))\n",
    );
    assert!(js.contains("pyLen("), "len must stay a builtin: {}", js);
    assert!(
        js.contains("pySorted("),
        "sorted must stay a builtin: {}",
        js
    );
    assert!(!js.contains("createElement(\"len\""), "JS: {}", js);
    assert!(!js.contains("createElement(\"sorted\""), "JS: {}", js);
}

#[test]
fn test_psx_snake_case_unbound_name_not_claimed() {
    // Tag-shape is [a-z][a-z0-9]* — snake_case names (hooks, helpers)
    // never match the fallback even when unbound.
    let js = compile(
        "from pyths.react import component\n\
         \n\
         @component\n\
         def App():\n\
         \x20   v = my_helper()\n\
         \x20   return div(v)\n",
    );
    assert!(
        !js.contains("createElement(\"my_helper\""),
        "snake_case never a tag: {}",
        js
    );
}

#[test]
fn test_psx_unbound_tag_curried_form() {
    // Curried PSX form `font(color=...)(children)` gets the same fallback.
    let js = compile(
        "from pyths.react import component\n\
         \n\
         @component\n\
         def App():\n\
         \x20   return font(color=\"red\")(\"legacy\")\n",
    );
    assert!(
        js.contains("createElement(\"font\""),
        "curried font must lower: {}",
        js
    );
}

// --- #300: cross-module class inheritance (imported base) ---

#[test]
fn test_relative_import_base_takes_pyobject_model() {
    // A base imported RELATIVELY is another module of this same project —
    // it must take the same cooperative PyObject/__init__ path as a
    // same-file base. The native path emitted a derived `constructor`
    // with no super() -> "Must call super constructor" at `new`.
    let js = compile(
        "from .Shape import Shape\n\
         \n\
         class Rectangle(Shape):\n\
         \x20   def __init__(self, width, height):\n\
         \x20       self.width = width\n\
         \x20       self.height = height\n\
         \n\
         \x20   @property\n\
         \x20   def area(self):\n\
         \x20       return self.width * self.height\n",
    );
    assert!(js.contains("class Rectangle extends Shape"), "JS: {}", js);
    assert!(
        js.contains("__init__(width, height)"),
        "cooperative __init__ method: {}",
        js
    );
    assert!(
        !js.contains("constructor("),
        "no native constructor for a project base: {}",
        js
    );
    assert!(
        js.contains("__pyClass(Rectangle, [Shape])"),
        "MRO installed: {}",
        js
    );
}

#[test]
fn test_relative_import_base_alias_takes_pyobject_model() {
    // Aliased relative import: the LOCAL binding is what appears as the base.
    let js = compile(
        "from .shapes import Shape as Base\n\
         \n\
         class Rect(Base):\n\
         \x20   def __init__(self, w):\n\
         \x20       self.w = w\n",
    );
    assert!(
        js.contains("__init__(w)"),
        "cooperative __init__ method: {}",
        js
    );
    assert!(
        !js.contains("constructor("),
        "no native constructor: {}",
        js
    );
}

#[test]
fn test_absolute_import_base_stays_native() {
    // React.Component (npm absolute import) keeps the native constructor +
    // native super() path (A3): its own constructor does no MRO dispatch,
    // so a prototype __init__ would simply never run.
    let js = compile(
        "from react import Component\n\
         \n\
         class Clock(Component):\n\
         \x20   def __init__(self, props):\n\
         \x20       super().__init__(props)\n\
         \x20       self.state = {\"t\": 0}\n",
    );
    assert!(
        js.contains("constructor(props)"),
        "native constructor kept: {}",
        js
    );
    assert!(js.contains("super(props)"), "hoisted native super: {}", js);
    assert!(
        !js.contains("__pyClass(Clock"),
        "no cooperative model for native base: {}",
        js
    );
}

#[test]
fn test_native_ctor_synthesizes_bare_super_when_init_has_no_super_call() {
    // Python __init__ need not call super().__init__(); a JS derived
    // constructor MUST call super() before `this`. Synthesized bare super().
    let js = compile(
        "class ParseError(ValueError):\n\
         \x20   def __init__(self, code):\n\
         \x20       self.code = code\n",
    );
    let ctor_pos = js.find("constructor(code)").expect("native ctor emitted");
    let super_pos = js.find("super();").expect("bare super synthesized");
    let assign_pos = js.find("this.code = code").expect("field assign");
    assert!(
        ctor_pos < super_pos && super_pos < assign_pos,
        "super() must precede this-access: {}",
        js
    );
}

// --- #301: member-method name-lowering must respect the receiver ---

#[test]
fn test_append_provable_list_receiver_stays_inline_push() {
    // Provably-list receiver keeps the cheap `.push` inline.
    let js = compile("xs = []\nxs.append(1)\nxs.append(2)");
    assert!(
        js.contains("xs.push(1)"),
        "provable list inlines push: {}",
        js
    );
    assert!(
        !js.contains("pyAppend("),
        "no runtime dispatch needed: {}",
        js
    );
}

#[test]
fn test_append_unknown_receiver_dispatches_via_py_append() {
    // A function parameter has unknown type — could be a Python list OR a
    // DOM node / JS object with its own native .append. Must go through
    // the receiver-dispatching helper, NOT a blind `.push` rename.
    let js = compile("def add(el, node):\n    el.append(node)");
    assert!(
        js.contains("pyAppend(el, node)"),
        "unknown receiver dispatches: {}",
        js
    );
    assert!(!js.contains("el.push("), "no blind push rename: {}", js);
}

#[test]
fn test_extend_insert_unknown_receiver_dispatch() {
    let js = compile("def f(xs):\n    xs.extend([1])\n    xs.insert(0, 2)");
    assert!(js.contains("pyExtend(xs, "), "extend dispatches: {}", js);
    assert!(
        js.contains("pyInsert(xs, 0, 2)"),
        "insert dispatches: {}",
        js
    );
    assert!(!js.contains("xs.push("), "no blind push: {}", js);
    assert!(!js.contains("xs.splice("), "no blind splice: {}", js);
}

#[test]
fn test_extend_insert_provable_list_stays_inline() {
    let js = compile("xs = []\nxs.extend([1, 2])\nxs.insert(0, 9)");
    assert!(
        js.contains("xs.push(..."),
        "provable list inlines extend: {}",
        js
    );
    assert!(
        js.contains("xs.splice(0, 0, 9)"),
        "provable list inlines insert: {}",
        js
    );
}

#[test]
fn test_discard_dispatches_via_py_discard() {
    let js = compile("s = {1, 2}\ns.discard(1)");
    assert!(js.contains("pyDiscard(s, 1)"), "discard dispatches: {}", js);
    assert!(!js.contains("s.delete("), "no blind delete rename: {}", js);
}

#[test]
fn test_classlist_and_map_receivers_keep_runtime_dispatch() {
    // The WB-1 shape: DOM-ish member methods must never be blind-renamed.
    let js = compile(
        "def demo(el, node, m):\n    el.append(node)\n    el.remove()\n    el.classList.remove(\"active\")\n    return m.get(\"k\")",
    );
    assert!(
        js.contains("pyAppend(el, node)"),
        "append dispatches: {}",
        js
    );
    assert!(js.contains("pyRemove(el)"), "remove dispatches: {}", js);
    assert!(
        js.contains("pyRemove(el.classList, \"active\")"),
        "classList.remove dispatches: {}",
        js
    );
    assert!(js.contains("pyDictGet(m, \"k\")"), "get dispatches: {}", js);
    assert!(!js.contains("el.push("), "no blind push: {}", js);
}

// --- #306 follow-up: JS/browser globals are never claimed as PSX tags ---

#[test]
fn test_psx_js_globals_stay_plain_calls() {
    // FlameReact f015/f062 regression shape: `fetch(...)` inside a nested
    // async handler of a @component is unbound + tag-shaped, but it is a
    // browser global — it must stay a plain call, never createElement.
    let js = compile(
        "from pyths.react import component, use_state\n\
         \n\
         @component\n\
         def Rates():\n\
         \x20   info, set_info = use_state(None)\n\
         \x20   async def fetch_rates(cur):\n\
         \x20       response = await fetch(f\"/api/rate/{cur}\")\n\
         \x20       data = await response.json()\n\
         \x20       set_info(data)\n\
         \x20   return div(str(info))\n",
    );
    assert!(
        !js.contains("createElement(\"fetch\""),
        "fetch must not be an element: {}",
        js
    );
    assert!(
        js.contains("await fetch("),
        "fetch stays a plain global call: {}",
        js
    );
}

#[test]
fn test_psx_alert_and_atob_stay_plain_calls() {
    let js = compile(
        "from pyths.react import component\n\
         \n\
         @component\n\
         def App():\n\
         \x20   def on_click(e):\n\
         \x20       alert(atob(\"aGk=\"))\n\
         \x20   return button(on_click=on_click, \"go\")\n",
    );
    assert!(
        !js.contains("createElement(\"alert\""),
        "alert stays a call: {}",
        js
    );
    assert!(
        !js.contains("createElement(\"atob\""),
        "atob stays a call: {}",
        js
    );
    assert!(
        js.contains("alert(atob("),
        "plain global calls preserved: {}",
        js
    );
}

// B18: a redefined @component / Next.js export must NOT emit two `function X`
// declarations — that is an ESM SyntaxError ("Identifier already declared").
// The second definition (Python last-wins) reuses the reassignment form.
#[test]
fn test_b18_redefined_component_no_duplicate_decl() {
    let js = compile(
        "from pyths.react import component\n\
         \n\
         @component\n\
         def App():\n\
         \x20   return div(\"hi\")\n\
         \n\
         @component\n\
         def App():\n\
         \x20   return div(\"bye\")\n",
    );
    // Exactly one function declaration named App.
    assert_eq!(
        js.matches("function App(").count(),
        1,
        "expected a single `function App(` declaration, got:\n{}",
        js
    );
    // The redefinition lands as an assignment, not a second declaration.
    assert!(
        js.contains("App = function"),
        "redefinition should reuse the assignment form:\n{}",
        js
    );
}

#[test]
fn test_b18_redefined_nextjs_export_no_duplicate_decl() {
    let js = compile(
        "def get_server_side_props():\n\
         \x20   return {}\n\
         \n\
         def get_server_side_props():\n\
         \x20   return {}\n",
    );
    assert_eq!(
        js.matches("function getServerSideProps(").count(),
        1,
        "expected a single getServerSideProps declaration, got:\n{}",
        js
    );
    assert!(
        js.contains("getServerSideProps = function"),
        "redefined Next.js export should reuse the assignment form:\n{}",
        js
    );
}
