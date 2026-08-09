// PythScribe stdlib — Python `bisect` (binary search on a sorted list).
// Comparisons go through pyLt so tuple/custom keys order like CPython.

import { pyLt } from "../operators.js";

export function bisect_right(a, x, lo = 0, hi) {
    if (hi === undefined || hi === null) hi = a.length;
    while (lo < hi) {
        const mid = (lo + hi) >> 1;
        if (pyLt(x, a[mid])) hi = mid;
        else lo = mid + 1;
    }
    return lo;
}

export function bisect_left(a, x, lo = 0, hi) {
    if (hi === undefined || hi === null) hi = a.length;
    while (lo < hi) {
        const mid = (lo + hi) >> 1;
        if (pyLt(a[mid], x)) lo = mid + 1;
        else hi = mid;
    }
    return lo;
}

export function insort_right(a, x, lo = 0, hi) {
    a.splice(bisect_right(a, x, lo, hi), 0, x);
    return a;
}

export function insort_left(a, x, lo = 0, hi) {
    a.splice(bisect_left(a, x, lo, hi), 0, x);
    return a;
}

// Aliases (Python exposes both `bisect`/`bisect_right` and `insort`/`insort_right`).
export const bisect = bisect_right;
export const insort = insort_right;
