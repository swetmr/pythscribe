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
        // `frozenset(...)` → ReferenceError). Same canonicalizing factory;
        // immutability is not enforced (documented deviation).
        "frozenset" => Some(BuiltinMapping::Runtime("pySetOf")),
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
        "input" => Some(BuiltinMapping::Direct("prompt")),
        "repr" => Some(BuiltinMapping::Runtime("pyRepr")),
        // #166: value-aware runtime type() — primitives return interned
        // type objects with the CPython `__name__` ('int'/'str'/'bool'/
        // 'NoneType'/...); class instances still return their constructor.
        // The old Direct form gave Number/String/... whose __name__ is
        // undefined.
        "type" => Some(BuiltinMapping::Runtime("pyType")),
        _ => None,
    }
}

pub enum BuiltinMapping {
    /// Maps directly to a JS expression (no runtime import needed).
    Direct(&'static str),
    /// Maps to a runtime helper function.
    Runtime(&'static str),
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
        // Constructors with CPython zero-arg defaults.
        "int" => ("((...a) => (a.length === 0 ? 0 : pyInt(...a)))", &["pyInt"]),
        "float" => (
            "((...a) => (a.length === 0 ? 0 : pyFloat(...a)))",
            &["pyFloat"],
        ),
        "str" => (
            "((...a) => (a.length === 0 ? \"\" : pyStr(...a)))",
            &["pyStr"],
        ),
        "bool" => (
            "((...a) => (a.length === 0 ? false : pyBool(...a)))",
            &["pyBool"],
        ),
        "list" => ("((it) => (it === undefined ? [] : Array.from(it)))", &[]),
        "dict" => ("pyDict", &["pyDict"]),
        // #297: canonicalizing PySet factory (handles the zero-arg form).
        "set" => ("pySetOf", &["pySetOf"]),
        "frozenset" => ("pySetOf", &["pySetOf"]),
        "tuple" => ("pyTupleOf", &["pyTupleOf"]),
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
        "pow" => ("pyPow", &["pyPow"]),
        // Runtime helper (multi-iterable map(f, xs, ys) support).
        "map" => ("pyMap", &["pyMap"]),
        "filter" => ("((fn, iter) => [...iter].filter((x) => fn(x)))", &[]),
        // #348: lazy short-circuit consumption (see call-form note above).
        "any" => ("pyAny", &["pyAny"]),
        "all" => ("pyAll", &["pyAll"]),
        _ => return None,
    })
}
