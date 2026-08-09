// PythScribe stdlib — Python `heapq` (binary min-heap over a list, in place).
//
// Comparisons go through pyLt so tuple/list priorities order lexicographically
// like CPython (`(priority, item)` is the canonical heap element), not by JS
// array string-coercion.

import { pyLt } from "../operators.js";

function _siftdown(heap, startpos, pos) {
    const newitem = heap[pos];
    while (pos > startpos) {
        const parentpos = (pos - 1) >> 1;
        const parent = heap[parentpos];
        if (pyLt(newitem, parent)) {
            heap[pos] = parent;
            pos = parentpos;
        } else {
            break;
        }
    }
    heap[pos] = newitem;
}

function _siftup(heap, pos) {
    const endpos = heap.length;
    const startpos = pos;
    const newitem = heap[pos];
    let childpos = 2 * pos + 1;
    while (childpos < endpos) {
        const rightpos = childpos + 1;
        if (rightpos < endpos && !pyLt(heap[childpos], heap[rightpos])) {
            childpos = rightpos;
        }
        heap[pos] = heap[childpos];
        pos = childpos;
        childpos = 2 * pos + 1;
    }
    heap[pos] = newitem;
    _siftdown(heap, startpos, pos);
}

export function heappush(heap, item) {
    heap.push(item);
    _siftdown(heap, 0, heap.length - 1);
}

export function heappop(heap) {
    const lastelt = heap.pop();
    if (heap.length === 0) return lastelt;
    const returnitem = heap[0];
    heap[0] = lastelt;
    _siftup(heap, 0);
    return returnitem;
}

export function heapreplace(heap, item) {
    const returnitem = heap[0];
    heap[0] = item;
    _siftup(heap, 0);
    return returnitem;
}

export function heappushpop(heap, item) {
    if (heap.length && pyLt(heap[0], item)) {
        const t = heap[0];
        heap[0] = item;
        item = t;
        _siftup(heap, 0);
    }
    return item;
}

export function heapify(x) {
    const n = x.length;
    for (let i = (n >> 1) - 1; i >= 0; i--) _siftup(x, i);
}

export function nlargest(n, iterable, options) {
    const key = options && options.key ? options.key : null;
    const arr = [...iterable];
    // Sort a decorated copy descending by the (key or value), keep first n.
    const dec = arr.map((v, i) => [key ? key(v) : v, i, v]);
    dec.sort((a, b) => (pyLt(a[0], b[0]) ? 1 : pyLt(b[0], a[0]) ? -1 : a[1] - b[1]));
    return dec.slice(0, Math.max(0, n)).map((d) => d[2]);
}

export function nsmallest(n, iterable, options) {
    const key = options && options.key ? options.key : null;
    const arr = [...iterable];
    const dec = arr.map((v, i) => [key ? key(v) : v, i, v]);
    dec.sort((a, b) => (pyLt(a[0], b[0]) ? -1 : pyLt(b[0], a[0]) ? 1 : a[1] - b[1]));
    return dec.slice(0, Math.max(0, n)).map((d) => d[2]);
}

// `heapq.merge(*iterables, key=None, reverse=False)` — merge already-sorted
// inputs into one sorted iterator. The compiler lowers `key=`/`reverse=` to a
// trailing options object; a stable sort of the concatenation reproduces the
// k-way merge's output for sorted inputs (JS Array.sort is stable).
export function* merge(...iterables) {
    let key = null;
    let reverse = false;
    const last = iterables[iterables.length - 1];
    if (
        last !== null && typeof last === "object" && typeof last[Symbol.iterator] !== "function"
        && (Object.prototype.hasOwnProperty.call(last, "key")
            || Object.prototype.hasOwnProperty.call(last, "reverse"))
    ) {
        iterables.pop();
        if (last.key != null) key = last.key;
        reverse = !!last.reverse;
    }
    const all = [];
    for (const it of iterables) for (const v of it) all.push(v);
    all.sort((a, b) => {
        const ka = key ? key(a) : a;
        const kb = key ? key(b) : b;
        return pyLt(ka, kb) ? -1 : pyLt(kb, ka) ? 1 : 0;
    });
    if (reverse) all.reverse();
    yield* all;
}
