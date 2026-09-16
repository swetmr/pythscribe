// PythScribe standard library: operator module (#111).
// Thin first-class wrappers over the Python-semantics operator
// dispatchers, so `accumulate(xs, operator.mul)` / `sorted(pairs,
// key=operator.itemgetter(0))` work exactly like the corresponding
// infix forms.

import {
    pyAdd, pySub, pyMul, pyDiv, pyFloorDiv, pyMod, pyPow,
    pyEq, pyNe, pyLt, pyLe, pyGt, pyGe, pyNeg, pyAbs, pyTuple,
} from "../operators.js";
import { pyGetItem, pyContains } from "../runtime.js";
import { pyBool } from "../types.js";

export function add(a, b) { return pyAdd(a, b); }
export function sub(a, b) { return pySub(a, b); }
export function mul(a, b) { return pyMul(a, b); }
export function truediv(a, b) { return pyDiv(a, b); }
export function floordiv(a, b) { return pyFloorDiv(a, b); }
export function mod(a, b) { return pyMod(a, b); }
export function pow(a, b) { return pyPow(a, b); }

export function neg(a) { return pyNeg(a); }
export function pos(a) { return a; }
export function abs(a) { return pyAbs(a); }

export function eq(a, b) { return pyEq(a, b); }
export function ne(a, b) { return pyNe(a, b); }
export function lt(a, b) { return pyLt(a, b); }
export function le(a, b) { return pyLe(a, b); }
export function gt(a, b) { return pyGt(a, b); }
export function ge(a, b) { return pyGe(a, b); }

export function not_(a) { return !pyBool(a); }
export function truth(a) { return pyBool(a); }

export function getitem(obj, key) { return pyGetItem(obj, key); }
export function contains(container, item) { return pyContains(container, item); }
export function concat(a, b) { return pyAdd(a, b); }

/** itemgetter(k) / itemgetter(k1, k2, ...) — multi-key form returns a
 *  tuple, like CPython. */
export function itemgetter(...keys) {
    if (keys.length === 1) {
        const k = keys[0];
        return (obj) => pyGetItem(obj, k);
    }
    return (obj) => pyTuple(...keys.map((k) => pyGetItem(obj, k)));
}

/** attrgetter("name") / attrgetter("a.b") / multi-name form. */
export function attrgetter(...names) {
    const get = (obj, name) => name.split(".").reduce((o, part) => o[part], obj);
    if (names.length === 1) {
        const n = names[0];
        return (obj) => get(obj, n);
    }
    return (obj) => pyTuple(...names.map((n) => get(obj, n)));
}

//# sourceMappingURL=operator.js.map
