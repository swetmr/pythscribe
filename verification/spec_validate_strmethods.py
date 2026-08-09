#!/usr/bin/env python3
"""Stage-3 spec validation (lean-spec-quality pipeline) for Tier-3 wave 19 —
general string methods over code-point strings.

Faithful Python transliterations of the wave-19 Lean model functions
(verification/PythExpandVerify.lean, `sm*`/`js16FindSub`) are differentially
checked against REAL CPython `str` methods on a deterministic boundary-heavy
corpus (empty strings, empty needles, overlapping needles, astral code
points, lone surrogates). Per the skill: a counterexample here means the
SPEC or the CODE is wrong — do not prove until this is green.

Two arms:
  1. model-vs-CPython  — every wave-19 model function against the builtin
     (`startswith`/`endswith`/`in`/`find`/`index`/`count`/`replace`/
     `lstrip`/`rstrip`/`strip`/`split`/`join`).
  2. js16-vs-encoding  — the naive-JS deviation model `js16FindSub`
     (weighted code-point offsets) against the GROUND-TRUTH UTF-16
     code-unit search (encode both sides, first sublist occurrence — what
     `String.prototype.indexOf` actually computes). Lone surrogates ARE
     included: an unpaired surrogate is a well-defined SINGLE UTF-16 code
     unit (Python `surrogatepass`), so JS `indexOf` on it is well-defined —
     `js16FindSub` (1 unit for any cp < 0x10000) and the ground-truth
     encoder (surrogate passed through as one unit) must agree there too.
     The earlier "ill-defined encoding" rationale was wrong; the corrected
     arm now exercises lone surrogates instead of skipping them.

Usage: python verification/spec_validate_strmethods.py
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


# BMP letters, astral chars (𝔸 U+1D538, 💩 U+1F4A9), and a lone surrogate.
ALPHA_BMP = [0x61, 0x62, 0x63, 0x78]          # a b c x
ALPHA_ASTRAL = [0x1D538, 0x1F4A9]
LONE_SURROGATE = 0xD800

# --- wave-19 Lean model, transliterated ----------------------------------
# Each function mirrors the Lean definition STRUCTURALLY (same recursion),
# so agreement with CPython validates the Lean spec, not a paraphrase.

def sm_is_prefix(needle: list[int], hay: list[int]) -> bool:
    """List.isPrefixOf needle hay."""
    if not needle:
        return True
    if not hay:
        return False
    return needle[0] == hay[0] and sm_is_prefix(needle[1:], hay[1:])


def sm_starts_with(hay, pre):                       # s.startswith(pre)
    return sm_is_prefix(pre, hay)


def sm_ends_with(hay, suf):                         # s.endswith(suf)
    return sm_is_prefix(list(reversed(suf)), list(reversed(hay)))


def sm_find_sub(hay: list[int], needle: list[int]):
    """smFindSub: first code-point offset where needle is a prefix; None if absent."""
    if sm_is_prefix(needle, hay):
        return 0
    if not hay:
        return None
    r = sm_find_sub(hay[1:], needle)
    return None if r is None else r + 1


def sm_contains(hay, needle):                       # needle in s
    return sm_find_sub(hay, needle) is not None


def sm_count_aux(needle: list[int], hay: list[int], skip: int) -> int:
    if not hay:
        return 0
    if skip > 0:
        return sm_count_aux(needle, hay[1:], skip - 1)
    if sm_is_prefix(needle, hay):
        return 1 + sm_count_aux(needle, hay[1:], len(needle) - 1)
    return sm_count_aux(needle, hay[1:], 0)


def sm_count(hay, needle):                          # s.count(t)
    if not needle:
        return len(hay) + 1
    return sm_count_aux(needle, hay, 0)


def sm_replace_empty(repl: list[int], hay: list[int]) -> list[int]:
    if not hay:
        return list(repl)
    return list(repl) + [hay[0]] + sm_replace_empty(repl, hay[1:])


def sm_replace_aux(needle, repl, hay, skip) -> list[int]:
    if not hay:
        return []
    if skip > 0:
        return sm_replace_aux(needle, repl, hay[1:], skip - 1)
    if sm_is_prefix(needle, hay):
        return list(repl) + sm_replace_aux(needle, repl, hay[1:], len(needle) - 1)
    return [hay[0]] + sm_replace_aux(needle, repl, hay[1:], 0)


def sm_replace(hay, needle, repl):                  # s.replace(old, new)
    if not needle:
        return sm_replace_empty(repl, hay)
    return sm_replace_aux(needle, repl, hay, 0)


def sm_lstrip(chars, hay):                          # s.lstrip(chars) — dropWhile
    i = 0
    while i < len(hay) and hay[i] in chars:
        i += 1
    return hay[i:]


def sm_rstrip(chars, hay):                          # s.rstrip(chars)
    return list(reversed(sm_lstrip(chars, list(reversed(hay)))))


def sm_strip(chars, hay):                           # s.strip(chars)
    return sm_rstrip(chars, sm_lstrip(chars, hay))


def sm_split_skip(sep, hay, skip) -> list[list[int]]:
    if not hay:
        return [[]]
    if skip > 0:
        return sm_split_skip(sep, hay[1:], skip - 1)
    if sm_is_prefix(sep, hay):
        return [[]] + sm_split_skip(sep, hay[1:], len(sep) - 1)
    rest = sm_split_skip(sep, hay[1:], 0)
    return [[hay[0]] + rest[0]] + rest[1:]


def sm_split(hay, sep):                             # s.split(sep); [] sep → ValueError
    if not sep:
        return None
    return sm_split_skip(sep, hay, 0)


def sm_join(sep, pieces: list[list[int]]) -> list[int]:  # sep.join(pieces)
    if not pieces:
        return []
    if len(pieces) == 1:
        return list(pieces[0])
    return list(pieces[0]) + list(sep) + sm_join(sep, pieces[1:])


# --- naive-JS deviation model + UTF-16 ground truth ----------------------

def utf16_units(cp: int) -> int:
    return 2 if cp >= 0x10000 else 1


def utf16_encode(cps: list[int]) -> list[int]:
    out = []
    for cp in cps:
        if cp >= 0x10000:
            out.append(0xD800 + (cp - 0x10000) // 0x400)
            out.append(0xDC00 + (cp - 0x10000) % 0x400)
        else:
            out.append(cp)
    return out


def js16_find_sub(hay: list[int], needle: list[int]):
    """The Lean deviation model: Python match position re-weighted by UTF-16 widths."""
    if sm_is_prefix(needle, hay):
        return 0
    if not hay:
        return None
    r = js16_find_sub(hay[1:], needle)
    return None if r is None else r + utf16_units(hay[0])


def real_js_index_of(hay: list[int], needle: list[int]):
    """Ground truth for `String.prototype.indexOf`: first CODE-UNIT offset of
    the encoded needle inside the encoded haystack (-1 → None)."""
    h, n = utf16_encode(hay), utf16_encode(needle)
    for i in range(len(h) - len(n) + 1):
        if h[i:i + len(n)] == n:
            return i
    return None


# --- corpus + differential ----------------------------------------------

def cps_of(s: str) -> list[int]:
    return [ord(c) for c in s]


def str_of(cps: list[int]) -> str:
    return "".join(chr(c) for c in cps)


def gen_strings(rng, alphabet: list[int], n: int, maxlen: int) -> list[list[int]]:
    out: list[list[int]] = []
    for _ in range(n):
        ln = next(rng) % (maxlen + 1)
        out.append([alphabet[next(rng) % len(alphabet)] for _ in range(ln)])
    return out


def main() -> int:
    rng = xorshift(0x5EED_2026_0802)
    fails = 0
    checks = 0

    def check(label, model, cpython):
        nonlocal fails, checks
        checks += 1
        if model != cpython:
            nonlocal_fail = f"[FAIL] {label}: model={model!r} cpython={cpython!r}"
            print(nonlocal_fail)
            fails += 1

    # Arm 1: model vs CPython. Fixed boundary cases + randomized composites.
    alpha_full = ALPHA_BMP + ALPHA_ASTRAL + [LONE_SURROGATE]
    hays = ([[]] + gen_strings(rng, alpha_full, 220, 8)
            + [cps_of("ababab"), cps_of("aaa"), cps_of("𝔸x"), cps_of("𝔸𝔸x"),
               cps_of("a💩b"), cps_of("xxhixx"), cps_of("a,b,,c"), [0xD800, 0x61]])
    needles = ([[], [0x61], [0x61, 0x62], cps_of("ab"), cps_of("aa"),
                cps_of("𝔸"), cps_of("💩"), [0xD800]]
               + gen_strings(rng, alpha_full, 60, 3))
    # Force guaranteed-match needles: contiguous slices of each hay.
    sliced = []
    for h in hays[:80]:
        if h:
            lo = next(rng) % len(h)
            hi = lo + 1 + next(rng) % (len(h) - lo)
            sliced.append(h[lo:hi])
    needles += sliced

    for h in hays:
        s = str_of(h)
        for t in needles:
            u = str_of(t)
            check(f"startswith {h}/{t}", sm_starts_with(h, t), s.startswith(u))
            check(f"endswith {h}/{t}", sm_ends_with(h, t), s.endswith(u))
            check(f"contains {h}/{t}", sm_contains(h, t), u in s)
            check(f"find {h}/{t}", sm_find_sub(h, t),
                  None if s.find(u) == -1 else s.find(u))
            try:
                idx = s.index(u)
            except ValueError:
                idx = None
            check(f"index {h}/{t}", sm_find_sub(h, t), idx)
            check(f"count {h}/{t}", sm_count(h, t), s.count(u))
            check(f"replace {h}/{t}", str_of(sm_replace(h, t, cps_of("--"))),
                  s.replace(u, "--"))
            check(f"replace-self {h}/{t}", str_of(sm_replace(h, t, t)),
                  s.replace(u, u))
            if t:
                check(f"split {h}/{t}",
                      [str_of(p) for p in sm_split(h, t)], s.split(u))
            else:
                try:
                    s.split(u)
                    check(f"split-emptysep {h}", "no-error", "ValueError")
                except ValueError:
                    check(f"split-emptysep {h}", sm_split(h, t), None)

    # strip family: chars-set arm (explicit chars — the modeled scope).
    charsets = [cps_of("x"), cps_of("ab"), cps_of("𝔸"), cps_of("x𝔸"), []]
    for h in hays:
        s = str_of(h)
        for cs in charsets:
            c = str_of(cs)
            if not cs:
                # CPython: strip("") strips nothing.
                check(f"lstrip-empty {h}", str_of(sm_lstrip(cs, h)), s.lstrip(c))
                check(f"rstrip-empty {h}", str_of(sm_rstrip(cs, h)), s.rstrip(c))
                check(f"strip-empty {h}", str_of(sm_strip(cs, h)), s.strip(c))
                continue
            check(f"lstrip {h}/{cs}", str_of(sm_lstrip(cs, h)), s.lstrip(c))
            check(f"rstrip {h}/{cs}", str_of(sm_rstrip(cs, h)), s.rstrip(c))
            check(f"strip {h}/{cs}", str_of(sm_strip(cs, h)), s.strip(c))

    # join: sep.join(pieces) — including the split round trip on CPython.
    seps = [cps_of(","), cps_of("𝔸"), cps_of("ab"), []]
    for h in hays[:60]:
        s = str_of(h)
        for sep in seps:
            sp = str_of(sep)
            pieces = [h[:2], h[2:], []]
            check(f"join {h}/{sep}",
                  str_of(sm_join(sep, pieces)),
                  sp.join(str_of(p) for p in pieces))
            if sep:
                # CPython law the Lean round-trip theorem states: sep.join(s.split(sep)) == s
                check(f"split-join {h}/{sep}",
                      str_of(sm_join(sep, sm_split(h, sep))), s)

    # Arm 2: js16FindSub (weighted deviation model) vs true UTF-16 indexOf.
    # Lone surrogates INCLUDED: an unpaired surrogate is a valid single UTF-16
    # code unit, so `indexOf` on it is well-defined (not an ill-formed
    # encoding). `utf16_encode` passes it through as one unit and `js16_find_sub`
    # weights it as one unit — they must agree, and this arm now checks it.
    wf = hays
    wfn = needles
    saw_lone = False
    for h in wf:
        for t in wfn:
            if LONE_SURROGATE in h or LONE_SURROGATE in t:
                saw_lone = True
            checks += 1
            if js16_find_sub(h, t) != real_js_index_of(h, t):
                print(f"[FAIL] js16 {h}/{t}: model={js16_find_sub(h, t)} "
                      f"encoded-indexOf={real_js_index_of(h, t)}")
                fails += 1
    if not saw_lone:
        print("[FAIL] js16 arm never exercised a lone surrogate (corpus gap)")
        fails += 1

    if fails:
        print(f"[spec-validate/strmethods] {fails}/{checks} checks FAILED — "
              "fix the spec (or CPython drifted) before proving.")
        return 1
    print(f"[spec-validate/strmethods] all {checks} checks passed "
          "(model == CPython; js16 model == encoded indexOf)")
    return 0


if __name__ == "__main__":
    sys.setrecursionlimit(100000)
    sys.exit(main())
