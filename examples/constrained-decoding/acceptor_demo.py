#!/usr/bin/env python3
"""acceptor_demo.py — grammar-as-acceptor proof for the Repairability LOIR requirement.

A grammar-constrained decoder can only guarantee "the model cannot emit a
malformed program" if the grammar it constrains against is a *sound acceptor*:
it ACCEPTS every valid program and REJECTS every malformed one. This script
proves both halves offline, with no GPU and no model — the honest floor a live
constrained decoder (Tier 2) sits on top of.

Two experiments:

  Tier 1a — ACCEPT real model output (and never false-reject valid code).
    Parse every `.ps` / `.psc` completion from the baseline-001 generation eval
    with grammar/pyths.lark (LALR + PythonIndenter). `.psc` completions are
    expanded through `pyths expand` first, so the acceptor under test is exactly
    the canonical-.ps grammar a decoder would constrain. Model-error records
    (`<!-- model_error -->`, API timeouts — no program emitted) are not
    gradeable and are reported separately, not counted against the grammar.

    Crucially, every completion the grammar REJECTS is cross-checked against the
    AUTHORITATIVE parser (`pyths check`). If `pyths check` also rejects it, the
    completion was genuinely-invalid PythScribe (e.g. an inline suite) and the
    grammar rejecting it is *agreement*, not a false reject — precisely the
    malformed output a constrained decoder would have made unreachable. A
    grammar bug only exists if `pyths check` ACCEPTS something the grammar
    rejects (a false reject of valid code). We report that count; it must be 0.

  Tier 1b — REJECT malformed mutations.
    Take every accepted completion and apply a battery of guaranteed-malformed,
    structural mutations (drop a colon from a compound header, add a stray
    indent mid-block, unbalance a paren, inject a stray `$` outside strings,
    dangle a trailing operator). Each is constructed to be genuinely malformed —
    we deliberately avoid random char injection that could land inside a string
    literal or comment, where it would stay *valid* (a mutation bug, not a
    grammar bug). A sound acceptor rejects 100%.

Repro:
    pip install lark
    cargo build --release --workspace          # provides target/release/pyths
    python examples/constrained-decoding/acceptor_demo.py

Flags:
    --limit N     first N ps + N psc completions (smoke)
    --json PATH   write machine-readable summary
    --no-crosscheck   skip the `pyths check` corroboration of rejects (faster)
"""

import argparse
import json
import os
import re
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
EXE = ".exe" if sys.platform == "win32" else ""
PYTHS = os.environ.get("PYTHS_BIN", os.path.join(ROOT, "target", "release", f"pyths{EXE}"))
RAW = os.path.join(ROOT, "examples", "cloudflare-bench", "gen_eval", "raw", "baseline-001")
TMPDIR = os.path.join(ROOT, "examples", "constrained-decoding")

CODE_BLOCK = re.compile(r"```[a-zA-Z0-9_]*\n(.*?)```", re.DOTALL)


# --------------------------------------------------------------------------- #
# Grammar (mirrors scripts/test-grammar.py build_parsers()).
# --------------------------------------------------------------------------- #
def build_parser():
    try:
        import lark
        from lark.indenter import PythonIndenter
    except ImportError:
        print("FATAL: lark not installed — pip install lark", file=sys.stderr)
        sys.exit(2)
    return lark.Lark.open(
        os.path.join(ROOT, "grammar", "pyths.lark"),
        parser="lalr",
        postlex=PythonIndenter(),
        start="file_input",
        maybe_placeholders=False,
    )


def accepts(parser, source):
    try:
        parser.parse(source + "\n")
        return True
    except Exception:
        return False


def _run(cmd):
    return subprocess.run(cmd, capture_output=True, encoding="utf-8", errors="replace")


def _tmp_write(suffix, source):
    path = os.path.join(TMPDIR, f".acc_tmp{suffix}")
    with open(path, "w", encoding="utf-8", newline="\n") as f:
        f.write(source)
    return path


def pyths_expand(source):
    path = _tmp_write(".psc", source)
    try:
        r = _run([PYTHS, "expand", path])
    finally:
        _rm(path)
    return r.stdout if r.returncode == 0 else None


def pyths_check_accepts(source):
    """True iff the authoritative parser accepts `source` (canonical .ps)."""
    path = _tmp_write(".ps", source)
    try:
        r = _run([PYTHS, "check", path])
    finally:
        _rm(path)
    return r.returncode == 0


def _rm(path):
    try:
        os.remove(path)
    except OSError:
        pass


# --------------------------------------------------------------------------- #
def extract_code(md_text):
    if "<!-- model_error -->" in md_text:
        return None, "model_error (API timeout / mid-response close — no program emitted)"
    m = CODE_BLOCK.search(md_text)
    if not m:
        return None, "no fenced code block"
    return m.group(1), None


def load_completions(limit=None):
    out = []
    for cond in ("ps", "psc"):
        files = sorted(f for f in os.listdir(RAW) if f"_{cond}_" in f and f.endswith(".md"))
        if limit:
            files = files[:limit]
        for fn in files:
            with open(os.path.join(RAW, fn), encoding="utf-8") as f:
                code, err = extract_code(f.read())
            rec = {"name": fn, "cond": cond}
            if code is None:
                rec.update(gradeable=False, source=None, reason=err)
                out.append(rec)
                continue
            if cond == "psc":
                canonical = pyths_expand(code)
                if canonical is None:
                    rec.update(gradeable=False, source=None,
                               reason="pyths expand failed (invalid .psc)")
                    out.append(rec)
                    continue
                rec.update(gradeable=True, source=canonical)
            else:
                rec.update(gradeable=True, source=code)
            out.append(rec)
    return out


# --------------------------------------------------------------------------- #
# Tier 1b — guaranteed-malformed structural mutations.
# --------------------------------------------------------------------------- #
_HEADER = re.compile(r"^\s*(def|class|if|elif|else|for|while|with|try|except|finally)\b")


def _indent_of(ln):
    return len(ln) - len(ln.lstrip(" "))


def mutations(source):
    lines = source.split("\n")

    # 1. drop_colon — strip trailing ':' from the first compound header.
    for i, ln in enumerate(lines):
        if _HEADER.match(ln) and ln.rstrip().endswith(":"):
            m = lines[:]
            m[i] = ln.rstrip()[:-1]
            yield ("drop_colon", "\n".join(m))
            break

    # 2. missing_indent — strip ALL indentation from the first body line after a
    #    compound header ending ':'. A ':'-header is always at statement level
    #    (never inside brackets), so its suite MUST be indented; de-indenting the
    #    body to column 0 is a guaranteed "Expected INDENT". This is bracket-safe,
    #    unlike naively adding a space to a line that may sit inside an open '('
    #    (implicit line continuation) where indentation is insignificant.
    for i in range(len(lines) - 1):
        h = lines[i]
        if _HEADER.match(h) and h.rstrip().endswith(":"):
            # next non-blank line is the body head
            j = i + 1
            while j < len(lines) and not lines[j].strip():
                j += 1
            if j < len(lines) and _indent_of(lines[j]) > 0:
                m = lines[:]
                m[j] = lines[j].lstrip(" ")
                yield ("missing_indent", "\n".join(m))
                break

    # 3. unbalanced_paren — delete the first '(' on a line with NO quote char
    #    (avoids string interiors, so the imbalance is real).
    for i, ln in enumerate(lines):
        if "(" in ln and '"' not in ln and "'" not in ln:
            j = ln.index("(")
            m = lines[:]
            m[i] = ln[:j] + ln[j + 1:]
            yield ("unbalanced_paren", "\n".join(m))
            break

    # 4. stray_dollar — insert '$' at the start of code content on the first
    #    line with no quote and no '#' (outside any string/comment) -> '$' is
    #    not a valid canonical-.ps token.
    for i, ln in enumerate(lines):
        s = ln.strip()
        if s and '"' not in ln and "'" not in ln and "#" not in ln:
            ind = _indent_of(ln)
            m = lines[:]
            m[i] = ln[:ind] + "$" + ln[ind:]
            yield ("stray_dollar", "\n".join(m))
            break

    # 5. dangling_op — trailing binary operator at EOF (truncated expression),
    #    the canonical "the model got cut off mid-token" failure a decoder stops.
    yield ("dangling_op", source.rstrip() + " +")


# --------------------------------------------------------------------------- #
def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--limit", type=int, default=None)
    ap.add_argument("--json", default=None)
    ap.add_argument("--no-crosscheck", action="store_true")
    args = ap.parse_args()

    if not os.path.exists(PYTHS):
        print(f"FATAL: {PYTHS} missing — cargo build --release --workspace", file=sys.stderr)
        sys.exit(2)

    parser = build_parser()
    print("grammar : grammar/pyths.lark  (LALR + PythonIndenter, lark)")
    print(f"corpus  : {os.path.relpath(RAW, ROOT)}")
    print(f"parser  : {os.path.relpath(PYTHS, ROOT)} (authoritative cross-check)")
    comps = load_completions(args.limit)

    # ---- Tier 1a ---------------------------------------------------------- #
    stat = {c: {"gradeable": 0, "accept": 0, "ungradeable": 0,
                "corroborated_reject": [], "false_reject": []} for c in ("ps", "psc")}
    accepted = []
    for c in comps:
        b = stat[c["cond"]]
        if not c["gradeable"]:
            b["ungradeable"] += 1
            continue
        b["gradeable"] += 1
        if accepts(parser, c["source"]):
            b["accept"] += 1
            accepted.append(c)
        else:
            # Grammar rejected. Ask the authoritative parser who's right.
            if args.no_crosscheck:
                b["false_reject"].append((c["name"], "grammar rejected (not cross-checked)"))
            elif pyths_check_accepts(c["source"]):
                b["false_reject"].append((c["name"], "pyths check ACCEPTS -> grammar gap"))
            else:
                b["corroborated_reject"].append(c["name"])

    print("\n=== Tier 1a — ACCEPT real model output ===")
    tot_g = tot_a = tot_ung = 0
    all_false = []
    all_corrob = []
    for cond in ("ps", "psc"):
        b = stat[cond]
        g, a = b["gradeable"], b["accept"]
        rate = a / g * 100 if g else 0.0
        print(f"  {cond:4s}: {a}/{g} gradeable accepted ({rate:.1f}%)   "
              f"[{b['ungradeable']} ungradeable model-error records skipped]")
        for name in b["corroborated_reject"]:
            print(f"         reject OK  {name}: pyths check ALSO rejects "
                  f"(model emitted invalid PythScribe)")
        for name, why in b["false_reject"]:
            print(f"         FALSE REJECT {name}: {why}")
        tot_g += g; tot_a += a; tot_ung += b["ungradeable"]
        all_false += b["false_reject"]; all_corrob += b["corroborated_reject"]
    print(f"  ALL : {tot_a}/{tot_g} gradeable accepted "
          f"({tot_a / tot_g * 100 if tot_g else 0:.1f}%)   "
          f"[{tot_ung} model-error records skipped]")
    print(f"  grammar rejects corroborated by `pyths check` (invalid model output): {len(all_corrob)}")
    print(f"  FALSE rejects of valid code (real grammar gaps): {len(all_false)}")

    # ---- Tier 1b ---------------------------------------------------------- #
    per_label = {}
    leaks = []
    mut_total = mut_rej = 0
    for c in accepted:
        for label, mut in mutations(c["source"]):
            if mut == c["source"]:
                continue
            mut_total += 1
            d = per_label.setdefault(label, [0, 0])
            d[0] += 1
            if not accepts(parser, mut):
                mut_rej += 1
                d[1] += 1
            else:
                leaks.append((c["name"], label))

    print("\n=== Tier 1b — REJECT malformed mutations ===")
    print(f"  mutations tested : {mut_total}  (over {len(accepted)} accepted completions)")
    print(f"  rejected         : {mut_rej}/{mut_total} "
          f"({mut_rej / mut_total * 100 if mut_total else 0:.2f}%)")
    for label in sorted(per_label):
        t, r = per_label[label]
        print(f"      {label:18s}: {r}/{t} rejected ({r / t * 100 if t else 0:.1f}%)")
    if leaks:
        print(f"  LEAKS (grammar accepted a malformed mutation): {len(leaks)}")
        for name, label in leaks[:20]:
            print(f"      LEAK {name} [{label}]")

    # ---- verdict ---------------------------------------------------------- #
    accept_ok = len(all_false) == 0
    reject_ok = mut_rej == mut_total
    print("\n=== VERDICT ===")
    print(f"  no false rejects of valid code : {accept_ok}  "
          f"(gradeable accept {tot_a}/{tot_g}; {len(all_corrob)} rejects corroborated as invalid)")
    print(f"  rejects 100% of malformed muts : {reject_ok}  ({mut_rej}/{mut_total})")
    print("  => grammar/pyths.lark is a SOUND ACCEPTOR (no valid program rejected, "
          "no malformed program accepted)"
          if accept_ok and reject_ok else
          "  => acceptor NOT sound on this corpus — see FALSE REJECT / LEAK lines")

    if args.json:
        with open(args.json, "w", encoding="utf-8") as f:
            json.dump({
                "grammar": "grammar/pyths.lark",
                "corpus": "baseline-001",
                "tier1a": {c: {"gradeable": stat[c]["gradeable"],
                               "accept": stat[c]["accept"],
                               "ungradeable": stat[c]["ungradeable"],
                               "corroborated_reject": stat[c]["corroborated_reject"],
                               "false_reject": [n for n, _ in stat[c]["false_reject"]]}
                           for c in ("ps", "psc")},
                "tier1a_all": {"gradeable": tot_g, "accept": tot_a,
                               "ungradeable": tot_ung,
                               "corroborated_reject": len(all_corrob),
                               "false_reject": len(all_false)},
                "tier1b": {"total": mut_total, "rejected": mut_rej,
                           "per_label": per_label, "leaks": leaks},
                "sound_acceptor": bool(accept_ok and reject_ok),
            }, f, indent=2)
        print(f"\nwrote {args.json}")

    sys.exit(0 if (accept_ok and reject_ok) else 1)


if __name__ == "__main__":
    main()
