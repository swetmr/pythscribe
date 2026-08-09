// PythScribe Runtime — Core (Worker / tree-shake-safe) subpath
// ============================================================
// Exposes ONLY numeric and collection helpers.
// Zero references to dom.js, react.js, document, or window.
//
// Transitive import graph:
//   core.js → operators.js → runtime.js   (no further imports)
//   core.js → runtime.js                  (no further imports)
//   core.js → types.js                    (no further imports)
//   core.js → classes.js                  (no further imports)
//
// Safe to import in Cloudflare Workers, Deno, WASM-host environments, and
// any context where browser globals are unavailable.
//
// Fix: B-030 — pyths-runtime not Worker/tree-shake-safe for numeric-only output.

// ── Operators ─────────────────────────────────────────────────────────────
export {
    pyEq, pyAdd, pySub, pyMul, pyDiv, pyFloorDiv, pyMod, pyPow,
    pyLt, pyLe, pyGt, pyGe, pyNe, pyNeg, pyAbs,
    pyStr, pyRepr, pyPrint, pyTuple, pyTupleOf, pyFormatFloat,
    // bitwise family — needed by the edge/worker target (`--target worker`
    // imports runtime helpers from `pyths-runtime/core`); previously ONLY the
    // package/inline paths exported these, so a worker build using `|`/`&`/`^`/
    // `<<`/`>>`/`~` failed at ES-module instantiation (surfaced by wave-14's
    // pyBitNot). All six are exported here for target parity.
    pyBitOr, pyBitAnd, pyBitXor, pyShiftLeft, pyShiftRight, pyBitNot,
} from "./operators.js";

// ── Core builtins + exception classes + collection method helpers ─────────
export {
    // builtins
    pyRange, pyEnumerate, pyZip, pyMap, pySorted, pyReversed,
    pyLen, pyIter, pySlice, pySetSlice, pyContains, pyRound, pyGetItem,
    // exceptions
    ValueError, IndexError, KeyError, AttributeError,
    StopIteration, ZeroDivisionError, Exception,
    NameError, UnboundLocalError, __UNBOUND, __pyChkLocal, __pyChkGlobal,
    // format / string method helpers
    pyFormatSpec, pyFormatDynamic,
    pyStrJoin, pyStrSplit, pyStrTitle, pyStrCapitalize, pyStrFormat,
    pyStrStrip, pyStrLstrip, pyStrRstrip,
    pyStrCenter, pyStrLjust, pyStrRjust, pyStrExpandtabs,
    pyStrPartition, pyStrRpartition, pyStrRsplit, pyStrSplitlines,
    pyStrSwapcase, pyStrTranslate, pyStrIsidentifier, pyStrIsprintable,
    pyStrIstitle, pyStrRindex,
    // list / dict / set method helpers
    pyListRemove, pyListCount, pyListClear, pyListSort,
    pyIndex, pyPop,
    pyDictGet, pyDictSetdefault, pyDictPopitem,
    pySetItem, pyDictKeys, pyDictValues, pyDictItems, pyDictMerge, pyDict,
    pyCount, pyClear, pyCopy, pyRemove, pyUpdate, pyNormalizeStyle,
    pyAppend, pyExtend, pyInsert, pyFind, pyDiscard,
    pySetUnion, pySetIntersection, pySetDifference, pySetSymmetricDifference,
    pySetIntersectionUpdate, pySetDifferenceUpdate, pySetSymmetricDifferenceUpdate,
    pySetIsdisjoint, pySetIssubset, pySetIssuperset,
    pySetOf, // #297: canonicalizing set() / frozenset() factory
} from "./runtime.js";

// ── Python container types ────────────────────────────────────────────────
export { pyBool, PyDict, PySet, PyTuple } from "./types.js";

// ── Cooperative MRO object model ──────────────────────────────────────────
export { PyObject, __pyC3, __pyClass, __pySuper, __pyIsInstance } from "./classes.js";
