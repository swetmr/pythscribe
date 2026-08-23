/// Map Python builtin function names to their JS equivalents.
pub fn builtin_func_mapping(name: &str) -> Option<BuiltinMapping> {
    match name {
        "print" => Some(BuiltinMapping::Runtime("pyPrint")),
        "len" => Some(BuiltinMapping::Runtime("pyLen")),
        "range" => Some(BuiltinMapping::Runtime("pyRange")),
        "enumerate" => Some(BuiltinMapping::Runtime("pyEnumerate")),
        "zip" => Some(BuiltinMapping::Runtime("pyZip")),
        "sorted" => Some(BuiltinMapping::Runtime("pySorted")),
        "reversed" => Some(BuiltinMapping::Runtime("pyReversed")),
        // F5: 1-arg next(gen) → gen.next().value, raising StopIteration when
        // exhausted. (2-arg next(gen, default) honored incidentally but stays
        // officially unsupported — B-011.)
        "next" => Some(BuiltinMapping::Runtime("pyNext")),
        "iter" => Some(BuiltinMapping::Runtime("pyIter")),
        "isinstance" => Some(BuiltinMapping::Runtime("__pyIsInstance")),
        // autotester classes: issubclass over compiled classes (__mro__ /
        // prototype chain), interned builtin type objects, and tuples.
        "issubclass" => Some(BuiltinMapping::Runtime("__pyIsSubclass")),
        // A4: was Direct("String") — JS's String() gives JS formatting
        // (String(true) === "true", String([1,2]) === "1,2"). Route
        // through the runtime's Python-semantics str() instead (see
        // pyStr in runtime/src/operators.js).
        "str" => Some(BuiltinMapping::Runtime("pyStr")),
        // #82: was Direct Math.trunc(Number(x)) — int("abc") silently
        // returned NaN. pyInt validates strings and raises CPython-shaped
        // ValueError; large numeric strings stay exact (BigInt-backed).
        "int" => Some(BuiltinMapping::Runtime("pyInt")),
        // F4: route through pyFloat so `float("inf")`/`float("-inf")`/
        // `float("nan")` map to real Infinity/-Infinity/NaN (Number("inf")
        // yields NaN), case-insensitively and whitespace-tolerant.
        "float" => Some(BuiltinMapping::Runtime("pyFloat")),
        "bool" => Some(BuiltinMapping::Runtime("pyBool")),
        // Was Direct("Array.from"): `Array.from(d)` on a plain-object dict
        // (all-string-key representation, no Symbol.iterator) silently returned
        // `[]` instead of the keys, and on a Map-backed dict returned entries,
        // not keys. pyListOf routes through pySeq (dict → keys, str → code
        // points, range/gen/set materialized) and copies. The empty-arg form
        // `list()` is still lowered directly to `[]` at the call site.
        "list" => Some(BuiltinMapping::Runtime("pyListOf")),
        // #83: pyDict is a FACTORY (the old mapping was the PyDict class,
        // which crashes when called without `new`). It shape-chooses at
        // runtime: all-string keys → plain object (also the documented
        // PyDict→plain-JS-object escape hatch for JS interop), any
        // non-string key → Map-backed PyDict.
        "dict" => Some(BuiltinMapping::Runtime("pyDict")),
        // #110/#297: `set(...)` routes through the pySetOf factory — it
        // builds the canonicalizing PySet (bool/int/float hash identity,
        // structural tuple membership) and iterates dicts as keys; the
        // zero-arg form gives an empty PySet. tuple() routes through the
        // pyTupleOf factory (tuples are marked arrays, not class instances).
        "set" => Some(BuiltinMapping::Runtime("pySetOf")),
        // #297: frozenset previously had NO mapping at all (bare
        // `frozenset(...)` → ReferenceError). Same canonicalizing storage,
        // now via pyFrozensetOf which BRANDS the result (`__pyfrozen__`) so
        // the in-place aug-assign helpers rebind instead of mutating it
        // (aliasing soundness); full immutability is still not enforced
        // (documented deviation).
        "frozenset" => Some(BuiltinMapping::Runtime("pyFrozensetOf")),
        // autotester byte_arrays: bytes()/bytearray() constructors.
        "bytes" => Some(BuiltinMapping::Runtime("pyBytesOf")),
        "bytearray" => Some(BuiltinMapping::Runtime("pyBytearrayOf")),
        "tuple" => Some(BuiltinMapping::Runtime("pyTupleOf")),
        // Runtime helper (not bare Math.abs) so custom-class operands
        // (Decimal/Fraction's `__abs__`) get the right type back instead
        // of being silently coerced to a plain float by Math.abs's
        // ToNumber conversion. Falls back to Math.abs unchanged for
        // everything else — see pyAbs in runtime/src/operators.js.
        "abs" => Some(BuiltinMapping::Runtime("pyAbs")),
        "round" => Some(BuiltinMapping::Runtime("pyRound")),
        // #88: was Direct Math.min/Math.max — correct only for the
        // multi-scalar-arg form; the single-iterable form returned NaN and
        // key= was unsupported. pyMin/pyMax handle both forms + key/default.
        "min" => Some(BuiltinMapping::Runtime("pyMin")),
        "max" => Some(BuiltinMapping::Runtime("pyMax")),
        // #94: was a reduce lambda with a hardcoded 0 seed — sum(xs, start)
        // silently discarded start (positional AND keyword forms).
        "sum" => Some(BuiltinMapping::Runtime("pySum")),
        // #89 / #90: chr/ord/divmod previously had no mapping at all and
        // crashed with ReferenceError at runtime.
        "chr" => Some(BuiltinMapping::Runtime("pyChr")),
        "ord" => Some(BuiltinMapping::Runtime("pyOrd")),
        "divmod" => Some(BuiltinMapping::Runtime("pyDivmod")),
        // #206: bin/hex/oct had no mapping and crashed with ReferenceError
        // (`bin is not defined`) — surfaced by HumanEval /79 /84 /103 /116.
        "bin" => Some(BuiltinMapping::Runtime("pyBin")),
        "hex" => Some(BuiltinMapping::Runtime("pyHex")),
        "oct" => Some(BuiltinMapping::Runtime("pyOct")),
        // #110: unary call — JS .map(fn) would pass (elem, index, array),
        // e.g. map(int, xs) fed the INDEX to pyInt as its base argument.
        // Now a runtime helper so the multi-iterable form map(f, xs, ys)
        // works (the extra iterables were dropped, feeding f `undefined`).
        "map" => Some(BuiltinMapping::Runtime("pyMap")),
        "filter" => Some(BuiltinMapping::Direct(
            "((fn, iter) => [...iter].filter((x) => fn(x)))",
        )),
        // #348: was Direct `[...iter].some/.every`, which materialises the
        // whole iterable before testing — breaks short-circuit semantics,
        // asymptotics, and OOMs on large/unbounded generators. pyAny/pyAll
        // consume the iterator lazily and early-return (the #155 lazy pattern).
        "any" => Some(BuiltinMapping::Runtime("pyAny")),
        "all" => Some(BuiltinMapping::Runtime("pyAll")),
        // public #3: `input` previously mapped to the browser-only `prompt`,
        // which is a bare ReferenceError on node/workers/edge — the exact
        // silent-crash class this issue closes. It is now DIAGNOSED at
        // compile time (see unsupported_builtin below) until a portable
        // lowering exists; browser code can call window.prompt explicitly.
        "repr" => Some(BuiltinMapping::Runtime("pyRepr")),
        // autotester: pow() routed through the BUILTIN helper — the pyPow
        // operator helper's 3rd parameter is the internal float-context flag,
        // so mapping the builtin onto it silently ignored the modulus
        // (pow(2, 10, 1000) → 1024, want 24). pyPowBuiltin delegates the
        // 2-arg form to pyPow and implements the modular 3-arg form.
        "pow" => Some(BuiltinMapping::Runtime("pyPowBuiltin")),
        // autotester attribs_by_name / callable_test: previously unmapped —
        // every use crashed with a bare ReferenceError at runtime.
        "getattr" => Some(BuiltinMapping::Runtime("pyGetattr")),
        "setattr" => Some(BuiltinMapping::Runtime("pySetattr")),
        "hasattr" => Some(BuiltinMapping::Runtime("pyHasattr")),
        "delattr" => Some(BuiltinMapping::Runtime("pyDelattr")),
        "callable" => Some(BuiltinMapping::Runtime("pyCallable")),
        // autotester general_functions: dir() on the compiled object model.
        "dir" => Some(BuiltinMapping::Runtime("pyDir")),
        // autotester properties: property(fget, fset) as a value in a class
        // body; __pyClassAttr installs the accessor pair.
        "property" => Some(BuiltinMapping::Runtime("pyProperty")),
        // autotester complex_numbers: the builtin complex() constructor
        // (0/1/2-arg numeric + the common string forms).
        "complex" => Some(BuiltinMapping::Runtime("pyComplexOf")),
        // #166: value-aware runtime type() — primitives return interned
        // type objects with the CPython `__name__` ('int'/'str'/'bool'/
        // 'NoneType'/...); class instances still return their constructor.
        // The old Direct form gave Number/String/... whose __name__ is
        // undefined.
        "type" => Some(BuiltinMapping::Runtime("pyType")),
        // public #3: format/slice/ascii/vars — the machinery already existed
        // (pyFormatSpec engine, PySlice, pyRepr, the compiled object model);
        // these were the cheap wins of the unimplemented-builtin sweep.
        // format(v[, spec]) — same engine as f-strings, __format__ protocol.
        "format" => Some(BuiltinMapping::Runtime("pyFormat")),
        // slice(stop) / slice(start, stop[, step]) — a real PySlice object;
        // pyGetItem dispatches it so `xs[slice(1, 3)]` ≡ `xs[1:3]`.
        "slice" => Some(BuiltinMapping::Runtime("pySliceOf")),
        // ascii(x) — repr with non-ASCII escaped (\xNN/\uNNNN/\UNNNNNNNN).
        "ascii" => Some(BuiltinMapping::Runtime("pyAscii")),
        // vars(obj) — the instance __dict__ as a dict. The ZERO-ARG form is
        // locals(), which has no compiled equivalent — emit_call rejects it
        // with a compile diagnostic before this mapping applies.
        "vars" => Some(BuiltinMapping::Runtime("pyVars")),
        // #448: `import_module(spec)` (importlib.import_module) lowers to a
        // NATIVE ES dynamic `import(spec)` call — a language form, not a
        // runtime helper. Documented deviation: importlib.import_module is
        // synchronous in CPython; ES `import()` returns a Promise, so in
        // `.ps` the result must be `await`ed (see docs/known-limitations).
        "import_module" => Some(BuiltinMapping::NativeCall("import")),
        _ => None,
    }
}

pub enum BuiltinMapping {
    /// Maps directly to a JS expression (no runtime import needed).
    Direct(&'static str),
    /// Maps to a runtime helper function.
    Runtime(&'static str),
    /// Emits a native JS call form `<keyword>(<args>)` with no runtime
    /// import — used for language constructs spelled as a call, e.g.
    /// `import_module(spec)` → `import(spec)` (dynamic import). The keyword
    /// is emitted verbatim, so it must be a valid call-expression head.
    NativeCall(&'static str),
}

/// #110: Python builtins referenced as VALUES (not called) — e.g.
/// `defaultdict(list)`, `starmap(pow)`, `sorted(xs, key=len)`,
/// `map(int, ...)`. Returns the JS expression for a first-class callable
/// with CPython call semantics — including the zero-arg constructor
/// defaults (`int()` → 0, `str()` → '', `list()` → [] ...), which are
/// exactly what `defaultdict` factories rely on — plus the runtime
/// helpers the expression needs. Names shadowed by user declarations
/// never reach this table (the caller guards on `!is_declared`).
pub fn builtin_value_mapping(name: &str) -> Option<(&'static str, &'static [&'static str])> {
    Some(match name {
        // Builtin TYPE names as values are the interned CALLABLE type
        // objects (runtime.js __pyType* singletons): still constructors
        // (map(int, xs), defaultdict(list)) but ALSO first-class types —
        // isinstance/issubclass second arg, `type(x) == int` identity,
        // `dict == dict` True (the old per-reference arrow literals made
        // fresh unequal functions), repr `<class 'int'>`. The constructor
        // helper stays in deps so the inline path keeps its hand-written
        // copy when present.
        "int" => ("__pyTypeInt", &["__pyTypeInt", "pyInt"]),
        "float" => ("__pyTypeFloat", &["__pyTypeFloat", "pyFloat"]),
        "str" => ("__pyTypeStr", &["__pyTypeStr", "pyStr"]),
        "bool" => ("__pyTypeBool", &["__pyTypeBool", "pyBool"]),
        "list" => ("__pyTypeList", &["__pyTypeList", "pyListOf"]),
        "dict" => ("__pyTypeDict", &["__pyTypeDict", "pyDict"]),
        "set" => ("__pyTypeSet", &["__pyTypeSet", "pySetOf"]),
        "frozenset" => ("__pyTypeFrozenset", &["__pyTypeFrozenset", "pyFrozensetOf"]),
        "tuple" => ("__pyTypeTuple", &["__pyTypeTuple", "pyTupleOf"]),
        // Bytes authority: first-class `bytes`/`bytearray` are the interned
        // callable type singletons too, so `type(b"") is bytes` and
        // `isinstance(x, (bytes, int))` hold like the other builtin types.
        "bytes" => ("__pyTypeBytes", &["__pyTypeBytes", "pyBytesOf"]),
        "bytearray" => ("__pyTypeBytearray", &["__pyTypeBytearray", "pyBytearrayOf"]),
        "object" => ("__pyTypeObject", &["__pyTypeObject"]),
        // Plain-function helpers — a direct reference is the callable.
        "print" => ("pyPrint", &["pyPrint"]),
        "len" => ("pyLen", &["pyLen"]),
        "range" => ("pyRange", &["pyRange"]),
        "enumerate" => ("pyEnumerate", &["pyEnumerate"]),
        "zip" => ("pyZip", &["pyZip"]),
        "sorted" => ("pySorted", &["pySorted"]),
        "reversed" => ("pyReversed", &["pyReversed"]),
        "abs" => ("pyAbs", &["pyAbs"]),
        "round" => ("pyRound", &["pyRound"]),
        "min" => ("pyMin", &["pyMin"]),
        "max" => ("pyMax", &["pyMax"]),
        "sum" => ("pySum", &["pySum"]),
        "chr" => ("pyChr", &["pyChr"]),
        "ord" => ("pyOrd", &["pyOrd"]),
        "divmod" => ("pyDivmod", &["pyDivmod"]),
        "iter" => ("pyIter", &["pyIter"]),
        "next" => ("pyNext", &["pyNext"]),
        "repr" => ("pyRepr", &["pyRepr"]),
        "type" => ("pyType", &["pyType"]),
        "pow" => ("pyPowBuiltin", &["pyPowBuiltin"]),
        "getattr" => ("pyGetattr", &["pyGetattr"]),
        "setattr" => ("pySetattr", &["pySetattr"]),
        "hasattr" => ("pyHasattr", &["pyHasattr"]),
        "delattr" => ("pyDelattr", &["pyDelattr"]),
        "callable" => ("pyCallable", &["pyCallable"]),
        "dir" => ("pyDir", &["pyDir"]),
        "property" => ("pyProperty", &["pyProperty"]),
        "complex" => ("pyComplexOf", &["pyComplexOf"]),
        // Runtime helper (multi-iterable map(f, xs, ys) support).
        "map" => ("pyMap", &["pyMap"]),
        "filter" => ("((fn, iter) => [...iter].filter((x) => fn(x)))", &[]),
        // #348: lazy short-circuit consumption (see call-form note above).
        "any" => ("pyAny", &["pyAny"]),
        "all" => ("pyAll", &["pyAll"]),
        // public #3: first-class references to the newly implemented four
        // (e.g. `map(ascii, xs)`, `key=hash`-style usage of format/vars).
        "format" => ("pyFormat", &["pyFormat"]),
        "slice" => ("pySliceOf", &["pySliceOf"]),
        "ascii" => ("pyAscii", &["pyAscii"]),
        "vars" => ("pyVars", &["pyVars"]),
        _ => return None,
    })
}

/// public #3 — the COMPLETE set of CPython builtin function names (the
/// "Built-in Functions" table of the Python 3.12 docs). This is the single
/// source of truth for the unimplemented-builtin compile diagnostic: a bare
/// Name that resolves to one of these, has no entry in the mapping tables
/// above, and no dedicated emitter path (see `EMITTER_SUPPORTED_BUILTINS`)
/// is a KNOWN builtin with NO implementation — emitting it verbatim would be
/// the silent compile-then-ReferenceError class this list closes.
/// (`True`/`False`/`None` are keywords, and exception types are handled by
/// the exception lowering — neither belongs here.)
pub const CPYTHON_BUILTIN_FUNCTIONS: &[&str] = &[
    "abs", "aiter", "all", "anext", "any", "ascii", "bin", "bool",
    "breakpoint", "bytearray", "bytes", "callable", "chr", "classmethod",
    "compile", "complex", "delattr", "dict", "dir", "divmod", "enumerate",
    "eval", "exec", "filter", "float", "format", "frozenset", "getattr",
    "globals", "hasattr", "hash", "help", "hex", "id", "input", "int",
    "isinstance", "issubclass", "iter", "len", "list", "locals", "map",
    "max", "memoryview", "min", "next", "object", "oct", "open", "ord",
    "pow", "print", "property", "range", "repr", "reversed", "round", "set",
    "setattr", "slice", "sorted", "staticmethod", "str", "sum", "super",
    "tuple", "type", "vars", "zip", "__import__",
];

/// Builtins supported through DEDICATED emitter paths rather than the
/// mapping tables: `super()` (zero-arg and two-arg MRO forms in method
/// bodies), and `@staticmethod`/`@classmethod` (class-body decorators).
/// They must not be flagged as unimplemented.
const EMITTER_SUPPORTED_BUILTINS: &[&str] = &["super", "staticmethod", "classmethod"];

/// public #3 DEEP FIX — true iff `name` is a KNOWN CPython builtin with NO
/// implementation anywhere in the compiler. Derived from the mapping tables
/// (not a second hand-maintained list), so implementing a builtin
/// automatically retires its diagnostic — the two can never drift.
pub fn unsupported_builtin(name: &str) -> bool {
    CPYTHON_BUILTIN_FUNCTIONS.contains(&name)
        && builtin_func_mapping(name).is_none()
        && builtin_value_mapping(name).is_none()
        && !EMITTER_SUPPORTED_BUILTINS.contains(&name)
}

/// The diagnostic for an unimplemented builtin — names the builtin, states
/// the deferral target, and (for the environment-shaped ones) points at the
/// interop escape hatch.
pub fn unsupported_builtin_message(name: &str) -> String {
    let hint = match name {
        "open" => " — file I/O is host-specific; use JS interop (fetch, node:fs) instead",
        "input" => " — use JS interop (window.prompt in the browser) instead",
        "globals" | "locals" => " — compiled output has no runtime namespace object",
        "eval" | "exec" | "compile" => {
            " — PythScribe is an ahead-of-time compiler and does not run arbitrary Python at runtime"
        }
        _ => "",
    };
    format!(
        "builtin '{}' is not supported yet (pythscribe-v3.x){}; \
         previously this compiled to a bare JS identifier and crashed at \
         runtime with ReferenceError",
        name, hint
    )
}
