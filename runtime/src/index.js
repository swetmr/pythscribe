// PythScribe Runtime — Main entry point
// Re-exports all runtime helpers.

export { pyRange, pyEnumerate, pyZip, pyMap, pySorted, pyReversed, pySeq, pyLen, pyIter, pyNext, pySlice, pySetSlice, pyContains, pyRound, pyChr, pyOrd, pyBin, pyHex, pyOct, pyMin, pyMax, pyDelItem, pyDelSlice } from "./runtime.js";
export { pyBool, pyAny, pyAll, pyAnd, pyOr, PyDict, PySet, PyTuple } from "./types.js";
export { pySetOf } from "./runtime.js";
export { pyEq, pyAdd, pySub, pyMul, pyDiv, pyFloorDiv, pyMod, pyPow, pyLt, pyLe, pyGt, pyGe, pyNe, pyNeg, pyAbs, pyStr, pyRepr, pyPrint, pyTuple, pyTupleOf, pyListOf, pyFormatFloat, pyFloat, pyInt, pyDivmod, pySum, pyBitOr, pyBitAnd, pyBitXor, pyBitNot, pyShiftLeft, pyShiftRight } from "./operators.js";
export { ValueError, IndexError, KeyError, AttributeError, StopIteration, StopAsyncIteration, ZeroDivisionError, Exception, OverflowError, RuntimeError, NotImplementedError, LookupError, ArithmeticError, NameError, UnboundLocalError, __UNBOUND, __pyChkLocal, __pyChkGlobal, pyGetItem, __pyAsyncIter, pyType } from "./runtime.js";
export { createPropTransformer, style, classes, component, psx } from "./react.js";
export { PyObject, __pyC3, __pyClass, __pySuper, __pyIsInstance, __pyClassAttr, __pyClassCall } from "./classes.js";

// Python-method runtime helpers (back the codegen's method_lowering table).
export {
    // existing
    pyFormatSpec, pyFormatDynamic, pyFixed,
    pyStrJoin, pyStrSplit, pyStrReplace, pyStrTitle, pyStrCapitalize, pyStrFormat,
    pyStrStrip, pyStrLstrip, pyStrRstrip,
    pyListRemove, pyListCount, pyListClear,
    pyIndex, pyPop,
    pyDictGet, pyDictSetdefault,
    // #83 — Map-backed dicts (non-string keys) + shape-dispatched helpers
    pySetItem, pyDictKeys, pyForIter, pyBoundMethod, pyDictValues, pyDictItems, pyDictMerge, pyDict, __pyCallKw, __pyKwArgs,
    // multi-receiver dispatchers
    pyCount, pyClear, pyCopy, pyRemove, pyUpdate, pyNormalizeStyle,
    // #301 — receiver-dispatched list/str/set methods (DOM/JS collisions)
    pyAppend, pyExtend, pyInsert, pyFind, pyDiscard,
    // generator protocol bridges (round-4 sweep)
    pyGenSend, pyGenClose, pyGenThrow,
    // additional string helpers
    pyStrCenter, pyStrLjust, pyStrRjust, pyStrExpandtabs,
    pyStrPartition, pyStrRpartition, pyStrRsplit, pyStrSplitlines,
    pyStrSwapcase, pyStrTranslate, pyStrIsidentifier, pyStrIsprintable,
    pyStrIstitle, pyStrIsupper, pyStrIslower, pyStrRindex, pyStrRfind,
    // list/dict
    pyListSort, pyDictPopitem,
    // set
    pySetUnion, pySetIntersection, pySetDifference, pySetSymmetricDifference,
    pySetIntersectionUpdate, pySetDifferenceUpdate, pySetSymmetricDifferenceUpdate,
    pySetIsdisjoint, pySetIssubset, pySetIssuperset,
} from "./runtime.js";
