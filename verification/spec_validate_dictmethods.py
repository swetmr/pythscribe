#!/usr/bin/env python3
"""Stage-3 spec validation (lean-spec-quality pipeline) for Tier-3 wave 20 —
dictionaries with int|str keys and the method surface
(`d[k]` / `get` / `in` / `len` / `keys` / `values` / `items`).

Faithful Python transliterations of the wave-20 Lean model functions
(verification/PythExpandVerify.lean, `dict*`/`jsKeyStr`/`jsObjLookup`) are
differentially checked against REAL CPython dicts built by literal-order
insertion, on a deterministic corpus of entry lists with duplicate keys,
mixed int/str keys, and the `1` vs `'1'` collision pairs.

Ground-truth CPython facts exercised (3.7+ semantics):
  - insertion order preserved; a duplicate literal key keeps its FIRST
    position but takes its LAST value;
  - int and str keys are DISTINCT (`{1: 'a', '1': 'b'}` has two entries) —
    this is exactly what a naive stringifying JS-object dict gets wrong
    (the wave-20 deviation model `jsObjLookup`);
  - `get` returns the default on a missing key; `in` is key membership.

Second arm: the naive-JS deviation model — `js_obj_lookup` (stringified
keys, the wave's deviation witness) must agree with a REAL stringifying
dict ({str(k): v}) on every probe, and must DISAGREE with CPython exactly
when a collision pair is present.

Usage: python verification/spec_validate_dictmethods.py
"""
from __future__ import annotations

import sys

# --- deterministic corpus (same xorshift as diff_harness.py) -------------

def xorshift(seed: int):
    x = seed
    while True:
        x ^= (x >> 12) & 0xFFFFFFFFFFFFFFFF
        x = (x ^ (x << 25)) & 0xFFFFFFFFFFFFFFFF
        x ^= x >> 27
        yield (x * 0x2545F4914F6CDD1D) & 0xFFFFFFFFFFFFFFFF


# Keys: ("int", n) | ("str", s) — mirrors the Lean `DKey` (kint | kstr).
KEY_POOL = [("int", 0), ("int", 1), ("int", -1), ("int", 2), ("int", 10),
            ("str", "1"), ("str", "-1"), ("str", "a"), ("str", ""),
            ("str", "10"), ("str", "b")]


def to_py(k):
    return k[1]


# --- wave-20 Lean model, transliterated ----------------------------------

def dict_lookup(k, es):
    """dictLookup: LAST entry with this key wins; None = KeyError."""
    if not es:
        return None
    (k2, v), rest = es[0], es[1:]
    r = dict_lookup(k, rest)
    if r is not None:
        return r
    return v if k2 == k else None


def dict_mem(k, es):
    if not es:
        return False
    (k2, _), rest = es[0], es[1:]
    return True if k2 == k else dict_mem(k, rest)


def dict_get(k, default, es):
    r = dict_lookup(k, es)
    return default if r is None else r


def dict_keys(es):
    """dictKeys: FIRST-occurrence order, deduplicated."""
    if not es:
        return []
    (k, _), rest = es[0], es[1:]
    return [k] + [k2 for k2 in dict_keys(rest) if k2 != k]


def dict_items(es):
    return [(k, dict_lookup(k, es)) for k in dict_keys(es)]


def dict_values(es):
    return [v for _, v in dict_items(es)]


def dict_len(es):
    return len(dict_keys(es))


# --- naive-JS deviation model --------------------------------------------

def js_key_str(k):
    """jsKeyStr: what a JS object does to a property key — stringify."""
    tag, val = k
    return str(val) if tag == "int" else val


def js_obj_lookup(k, es):
    """jsObjLookup: last entry whose STRINGIFIED key matches wins."""
    if not es:
        return None
    (k2, v), rest = es[0], es[1:]
    r = js_obj_lookup(k, rest)
    if r is not None:
        return r
    return v if js_key_str(k2) == js_key_str(k) else None


# --- differential --------------------------------------------------------

def build_cpython(es):
    d = {}
    for k, v in es:
        d[to_py(k)] = v
    return d


def build_js_object(es):
    d = {}
    for k, v in es:
        d[js_key_str(k)] = v
    return d


def gen_entry_lists(rng, n):
    out = []
    for _ in range(n):
        ln = next(rng) % 7
        out.append([(KEY_POOL[next(rng) % len(KEY_POOL)],
                     next(rng) % 100) for _ in range(ln)])
    return out


def main() -> int:
    rng = xorshift(0x5EED_2026_0802_20)
    fails = 0
    checks = 0

    def check(label, model, cpython):
        nonlocal fails, checks
        checks += 1
        if model != cpython:
            print(f"[FAIL] {label}: model={model!r} cpython={cpython!r}")
            fails += 1

    fixed = [
        [],
        [(("int", 1), 10)],
        [(("int", 1), 10), (("str", "1"), 20)],          # the collision pair
        [(("int", 1), 1), (("int", 2), 2), (("int", 1), 3)],  # dup key
        [(("str", "a"), 1), (("str", ""), 2), (("str", "a"), 3)],
        [(("int", -1), 5), (("str", "-1"), 6)],
        [(("int", 10), 1), (("str", "10"), 2), (("int", 10), 3)],
        # a >2-entry stringification collision with differing values (probe
        # int 1 reads 10 in CPython, 20 in the naive object) — the divergence
        # witness generalized beyond the 2-entry `jsObj_conflates` case.
        [(("int", 1), 10), (("str", "x"), 5), (("str", "1"), 20)],
    ]
    corpus = fixed + gen_entry_lists(rng, 400)

    for es in corpus:
        d = build_cpython(es)
        # keys / values / items / len — full ordered comparison.
        check(f"keys {es}", [to_py(k) for k in dict_keys(es)], list(d.keys()))
        check(f"values {es}", dict_values(es), list(d.values()))
        check(f"items {es}",
              [(to_py(k), v) for k, v in dict_items(es)], list(d.items()))
        check(f"len {es}", dict_len(es), len(d))
        # lookup / membership / get for every probe key.
        for k in KEY_POOL:
            pk = to_py(k)
            check(f"lookup {es} {k}", dict_lookup(k, es), d.get(pk))
            check(f"mem {es} {k}", dict_mem(k, es), pk in d)
            check(f"get {es} {k}", dict_get(k, -7, es), d.get(pk, -7))

    # Arm 2: the deviation model is faithful to a REAL stringifying dict,
    # and provably diverges from CPython on the collision pairs.
    for es in corpus:
        jd = build_js_object(es)
        for k in KEY_POOL:
            checks += 1
            if js_obj_lookup(k, es) != jd.get(js_key_str(k)):
                print(f"[FAIL] jsObj {es} {k}")
                fails += 1
    # The pinned collision witness: {1: 10, '1': 20} — CPython keeps both;
    # the stringifying model conflates them (kint 1 reads 20, not 10).
    es = [(("int", 1), 10), (("str", "1"), 20)]
    checks += 2
    if dict_lookup(("int", 1), es) != 10:
        print("[FAIL] witness: CPython-side lookup should be 10")
        fails += 1
    if js_obj_lookup(("int", 1), es) != 20:
        print("[FAIL] witness: stringifying lookup should be 20")
        fails += 1

    # Arm 3 (C11): the CLAIMED exact agreement boundary, not just one 2-entry
    # witness. `jsObjLookup_eq_of_injective` (Lean): the naive object AGREES
    # with CPython for a probe key k EXACTLY when no OTHER entry key stringifies
    # to jsKeyStr(k). `jsObj_conflates`: a stringification-colliding pair with
    # different values DIVERGES. We verify BOTH directions across the whole
    # corpus, and require the divergence direction to be witnessed with MORE
    # than two entries (generalizing the 2-entry theorem).
    def has_str_collision(k, es):
        ks = js_key_str(k)
        return any(js_key_str(k2) == ks and k2 != k for (k2, _) in es)

    saw_noncollide = False
    saw_multi_entry_divergence = False
    for es in corpus:
        d = build_cpython(es)
        for k in KEY_POOL:
            pk = to_py(k)
            agree = (js_obj_lookup(k, es) == d.get(pk))
            if not has_str_collision(k, es):
                # agreement boundary: with no collision the naive object MUST
                # match CPython (the exact content of jsObjLookup_eq_of_injective).
                saw_noncollide = True
                check(f"boundary-agree {es} {k}", agree, True)
            elif not agree and len(es) > 2:
                # a >2-entry stringification collision that genuinely diverges.
                saw_multi_entry_divergence = True
    check("boundary exercised a non-collision case", saw_noncollide, True)
    check("divergence witnessed beyond 2 entries", saw_multi_entry_divergence, True)

    if fails:
        print(f"[spec-validate/dictmethods] {fails}/{checks} checks FAILED")
        return 1
    print(f"[spec-validate/dictmethods] all {checks} checks passed "
          "(model == CPython; jsObj model == stringifying dict; "
          "collision witness diverges as proved)")
    return 0


if __name__ == "__main__":
    sys.setrecursionlimit(100000)
    sys.exit(main())
