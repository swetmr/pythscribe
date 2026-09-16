#!/usr/bin/env python3
"""WHOLE-TREE axiom-enforcement gate driver for the PythScribe verified core.

This is the ROOT authority that closes the review blocker: the prior gate was a
BYPASSABLE keyword grep (`ci.yml` trust-base step + `axiom_footprint.py ESCAPE_RE`)
whose positive axiom-subset check + nanoda re-check covered ONLY
`PythExpandVerify`'s import closure — the sibling proof modules
(Union7/Union8/C1C3C4Outcome/RoutingSoundness/RpcBoundary/C2TypeRepr/
C8HostInterop/EffectOrder/InternalConsistency/JsWasmMirror + generated *Data + Main)
were never whole-tree axiom-audited.

The gate proper is the Lean meta-program `verification/AxiomGate.lean` (exe
`axiomgate`). For the module set it is given it: re-imports them into a fresh env,
RE-TYPECHECKS every declaration through the KERNEL (`Environment.replay` —
lean4checker discipline, so the elaborator is not the TCB and a `set_option
debug.skipKernelTC true` decl is rejected), asserts axiom footprint ⊆
{propext, Classical.choice, Quot.sound} (Lean.collectAxioms), no user-declared
axiom, no `unsafe` (keyed on safety, never on a name), imports ⊆ Init∪covered, and
inventories `partial`/`implemented_by`/`extern`.

This driver:
  * DERIVES THE COVERED SET FROM WHAT WAS BUILT — every project `.olean` under
    `.lake/build/lib/lean` (recursive) — and REQUIRES it to equal the recursively
    discovered project `.lean` set. Neither a subdirectory module nor an
    oddly-named one nor a `.lean` that was not built can silently escape the audit
    (the removed grep only saw flat `*.lean`; this is strictly wider AND
    fail-closed — a stray/uncovered module is RED, not skipped);
  * runs the gate over that covered set (`gate`) and requires KERNEL_REPLAY ok;
  * runs the STRENGTH-LEDGER COMPLETENESS meta-check (`completeness`);
  * runs the ANTI-VACUITY paired negative-control self-test (`selftest`): each
    bypass form + a kernel-skip decl + a real `unsafe` + a prefixed/subdir shipping
    module all provably turn the gate/coverage RED; the clean tree PASSES.

Usage (from verification/):
    python comparator/whole_tree_axiom_gate.py gate           # CI gate (build + gate + completeness + coverage)
    python comparator/whole_tree_axiom_gate.py gate --no-build
    python comparator/whole_tree_axiom_gate.py completeness    # completeness/coverage meta-check only
    python comparator/whole_tree_axiom_gate.py selftest        # paired negative-control mutation drill

Exit codes: 0 = pass; 1 = a violation / coverage gap / self-test failure;
2 = usage error or a Lean/`lake` build failure.
"""
from __future__ import annotations

import os
import re
import shutil
import subprocess
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
VERIF = HERE.parent                       # verification/
LEAN_DIR = VERIF / ".lake" / "build" / "lib" / "lean"

# The gate meta-program itself is infrastructure (imports `Lean`, uses the meta
# API); it is NOT a shipping proof module and is excluded from the audited set.
INFRA_MODULES = {"AxiomGate"}
# Prefix the self-test writes its temporary modules with (for cleanup only — it is
# NOT a coverage exclusion: a built module with this prefix IS audited, fail-closed).
SELFTEST_PREFIX = "_agst"                 # case-insensitive match below


# ---- module discovery: BUILT (oleans) vs SOURCE (.lean), both recursive -----
def _module_name(path: Path, root: Path) -> str:
    return ".".join(path.relative_to(root).with_suffix("").parts)


def discover_built_modules():
    """Every built PROJECT module (recursive `.olean`), minus infra. This is what
    the gate can actually import and therefore what it audits."""
    mods = {}
    if not LEAN_DIR.exists():
        return mods
    for f in LEAN_DIR.rglob("*.olean"):
        name = _module_name(f, LEAN_DIR)
        if name in INFRA_MODULES:
            continue
        mods[name] = f
    return mods


def discover_source_modules():
    """Every project source module (recursive `.lean`, excluding the build tree),
    minus infra. Returns {module_name: path}."""
    mods = {}
    for f in VERIF.rglob("*.lean"):
        rel = f.relative_to(VERIF)
        if ".lake" in rel.parts:
            continue
        name = _module_name(f, VERIF)
        if name in INFRA_MODULES:
            continue
        mods[name] = f
    return mods


def covered_modules():
    """The set the gate audits = the built project oleans."""
    return sorted(discover_built_modules().keys())


def coverage_errors():
    """Built set must EQUAL source set — either side missing is a coverage gap.
    (A source `.lean` with no `.olean` = not built/covered; a stray `.olean` with
    no source = unexpected. Both are RED — the audit can never silently skip.)"""
    built = set(discover_built_modules())
    src = set(discover_source_modules())
    errs = []
    for m in sorted(src - built):
        errs.append(f"source module `{m}` has NO built .olean — it is not built/covered "
                    f"(add it as a lakefile target, or it escapes the axiom audit)")
    for m in sorted(built - src):
        errs.append(f"built module `{m}` has NO source .lean under verification/ — stray/unexpected olean")
    return errs


# ---- run the gate exe -------------------------------------------------------
def run_gate(mods, do_build=True):
    """Run the axiomgate exe over `mods` via `lake exe`. Returns (rc, stdout, stderr)."""
    if do_build:
        b = subprocess.run(["lake", "build", "axiomgate"], cwd=VERIF,
                           capture_output=True, text=True)
        if b.returncode != 0:
            sys.stderr.write("lake build axiomgate FAILED:\n" + b.stdout + b.stderr + "\n")
            return (2, "", b.stdout + b.stderr)
    # LEAN_ABORT_ON_PANIC makes a `partial`/`!` panic in the gate/replay fail loudly
    # (nonzero) instead of panic-and-continue (nit N2 — fail-closed for free).
    env = dict(os.environ, LEAN_ABORT_ON_PANIC="1")
    r = subprocess.run(["lake", "exe", "axiomgate", *mods], cwd=VERIF,
                       capture_output=True, text=True, env=env)
    return (r.returncode, r.stdout, r.stderr)


def parse_gate_output(stdout):
    """Return (modules, checked_thms, checked_decls, replay, replay_constants)."""
    modules, thms = set(), set()
    checked, replay, replay_n = 0, "unknown", -1
    for line in stdout.splitlines():
        line = line.strip()
        if line.startswith("MODULE "):
            parts = line.split()
            if len(parts) >= 2:
                modules.add(parts[1])
        elif line.startswith("CHECKED_THM "):
            thms.add(line[len("CHECKED_THM "):].strip())
        elif line.startswith("CHECKED_DECLS "):
            try:
                checked = int(line.split()[1])
            except (IndexError, ValueError):
                pass
        elif line.startswith("REPLAY_CONSTANTS "):
            try:
                replay_n = int(line.split()[1])
            except (IndexError, ValueError):
                pass
        elif line.startswith("KERNEL_REPLAY "):
            replay = line.split()[1] if len(line.split()) > 1 else "unknown"
    return modules, thms, checked, replay, replay_n


# ---- source theorem/lemma enumeration (recursive) ---------------------------
_BLOCK = re.compile(r"/-.*?-/", re.DOTALL)   # /- ... -/ and /-- ... -/ (non-nested; adequate)
_MODIF = r"(?:@\[[^\]]*\]\s*|(?:private|protected|noncomputable|scoped|local|unsafe|partial)\s+)*"
# A statement-position theorem/lemma: at line start (after attrs/modifiers), OR
# introduced by `set_option … in` / `open … in` (the S6 case). Precise enough to
# avoid matching `theorem` inside a string literal.
_DECL = re.compile(r"^\s*" + _MODIF + r"(?:theorem|lemma)\s+([A-Za-z_][A-Za-z0-9_'.]*)")
_DECL_IN = re.compile(r"\bin\s+" + _MODIF + r"(?:theorem|lemma)\s+([A-Za-z_][A-Za-z0-9_'.]*)")


def strip_block_comments(text):
    prev = None
    while prev != text:
        prev = text
        text = _BLOCK.sub("", text)
    return text


def source_theorems():
    """Map module -> list of top-level theorem/lemma SHORT names in the source."""
    out = {}
    for name, path in discover_source_modules().items():
        body = strip_block_comments(path.read_text(encoding="utf-8"))
        names = []
        for ln in body.splitlines():
            if ln.lstrip().startswith("--"):
                continue
            m = _DECL.search(ln) or _DECL_IN.search(ln)   # line-start OR `… in theorem x`
            if m:
                names.append(m.group(1).split(".")[-1])
        out[name] = names
    return out


def completeness_check(modules, thms, verbose=True):
    """Strength-ledger completeness: every discovered SOURCE module covered by the
    gate; every source theorem/lemma in the gate's checked set."""
    failures = []
    src_mods = set(discover_source_modules())
    for m in sorted(src_mods):
        if m not in modules:
            failures.append(f"module `{m}` is a shipping verification module but the gate did NOT report covering it")
    checked_short = {t.split(".")[-1] for t in thms}
    src = source_theorems()
    total = 0
    for stem, names in src.items():
        for nm in names:
            total += 1
            if nm not in checked_short:
                failures.append(f"theorem/lemma `{nm}` (source module `{stem}`) is NOT in the gate's checked set — coverage gap")
    if verbose:
        print(f"completeness: {len(src_mods)} source modules, {total} source theorems/lemmas, "
              f"{len(thms)} checked theorems reported by gate.")
    return failures


# ---------------------------------------------------------------- gate mode
def cmd_gate(do_build=True):
    if do_build:
        b = subprocess.run(["lake", "build"], cwd=VERIF, capture_output=True, text=True)
        if b.returncode != 0:
            sys.stderr.write("lake build FAILED:\n" + b.stdout + b.stderr + "\n")
            return 2
    print("== whole-tree axiom gate ==")
    cov_errs = coverage_errors()
    mods = covered_modules()
    print(f"covered {len(mods)} built modules: {' '.join(mods)}")
    rc, out, err = run_gate(mods, do_build=False)
    sys.stdout.write(out)
    if rc != 0:
        sys.stderr.write(err)
        if cov_errs:                       # NS3: surface coverage gaps even on a red gate
            print("\nCOVERAGE GAPS:")
            for c in cov_errs:
                print("  - " + c)
        print("\nGATE FAILED (see violations above).")
        return 1
    modules, thms, checked, replay, replay_n = parse_gate_output(out)
    failures = list(cov_errs)
    if replay != "ok":
        failures.append(f"KERNEL_REPLAY status is `{replay}` (expected ok) — the kernel re-check did not confirm")
    if replay_n != checked:                # NS2: replay must have been fed the whole audited set
        failures.append(f"REPLAY_CONSTANTS ({replay_n}) != CHECKED_DECLS ({checked}) — the kernel re-check did not cover every audited decl")
    failures += completeness_check(modules, thms)
    if failures:
        print("\nGATE/COVERAGE/COMPLETENESS FAILED:")
        for f in failures:
            print("  - " + f)
        return 1
    print("OK: whole-tree axiom gate (kernel-replayed) + coverage + completeness passed.")
    return 0


def cmd_completeness(do_build=True):
    if do_build:
        b = subprocess.run(["lake", "build"], cwd=VERIF, capture_output=True, text=True)
        if b.returncode != 0:
            sys.stderr.write("lake build FAILED:\n" + b.stdout + b.stderr + "\n")
            return 2
    cov_errs = coverage_errors()
    mods = covered_modules()
    rc, out, err = run_gate(mods, do_build=False)
    if rc != 0:
        sys.stdout.write(out)
        sys.stderr.write(err)
        print("completeness: gate itself failed — cannot assess completeness")
        return 1
    modules, thms, checked, replay, replay_n = parse_gate_output(out)
    failures = list(cov_errs) + completeness_check(modules, thms)
    if replay != "ok":
        failures.append(f"KERNEL_REPLAY status is `{replay}` (expected ok)")
    if replay_n != checked:
        failures.append(f"REPLAY_CONSTANTS ({replay_n}) != CHECKED_DECLS ({checked})")
    if failures:
        print("COMPLETENESS/COVERAGE FAILED:")
        for f in failures:
            print("  - " + f)
        return 1
    print("completeness + coverage OK.")
    return 0


# ---------------------------------------------------------------- self-test
def _compile_module(rel_lean: str) -> tuple[bool, str]:
    """Compile a standalone project module (possibly in a subdir) to its .olean in
    the search path, with the correct module name (root = verification/)."""
    src = VERIF / rel_lean
    modname = _module_name(src, VERIF)
    olean = LEAN_DIR / (Path(*modname.split(".")).with_suffix(".olean"))
    olean.parent.mkdir(parents=True, exist_ok=True)
    r = subprocess.run(["lake", "env", "lean", "-R", ".", str(src), "-o", str(olean)],
                       cwd=VERIF, capture_output=True, text=True)
    return (r.returncode == 0, r.stdout + r.stderr)


def _cleanup_selftest():
    """Remove every self-test temp source + olean (case-insensitive prefix), so
    discovery/coverage return to the clean tree. Fail-closed: leftovers would be RED,
    never silently skipped."""
    for f in list(VERIF.rglob("*.lean")):
        rel = f.relative_to(VERIF)
        if ".lake" in rel.parts:
            continue
        if rel.parts[0].lower().startswith(SELFTEST_PREFIX):
            try:
                shutil.rmtree(f.parent) if f.parent != VERIF else f.unlink()
            except (FileNotFoundError, OSError):
                pass
        elif f.stem.lower().startswith(SELFTEST_PREFIX):
            try:
                f.unlink()
            except FileNotFoundError:
                pass
    if LEAN_DIR.exists():
        for f in list(LEAN_DIR.rglob("*")):
            rel = f.relative_to(LEAN_DIR)
            if rel.parts and rel.parts[0].lower().startswith(SELFTEST_PREFIX):
                try:
                    (shutil.rmtree(f) if f.is_dir() else f.unlink())
                except (FileNotFoundError, OSError):
                    pass


# Every hard bypass form + a real unsafe (M1) in one standalone victim module.
VICTIM = "_agstVictim.lean"
VICTIM_SRC = """import Lean
axiom agst_plain_axiom : False
private axiom agst_private_axiom : False
@[simp] axiom agst_simp_axiom : (0 : Nat) = 0
theorem agst_sorry_thm : True := sorry
theorem agst_native_thm : (2 + 2 = 4) := by native_decide
unsafe def agst_unsafe_def : Nat := 0
-- M1: an unsafe def whose NAME ends in _unsafe_rec must NOT be exempted (safety-keyed)
unsafe def agst_forged_unsafe_rec : Nat := agst_forged_unsafe_rec
-- M1b: an unsafe opaque must NOT be laundered as a benign partial
unsafe opaque agst_unsafe_opaque : Nat := 0
-- a genuine (benign) partial def, inventoried not failed
partial def agst_partial_def (n : Nat) : Nat := agst_partial_def n
"""

# M2: a `False` theorem elaborated with the kernel type-checker DISABLED — the
# elaborator writes it, the kernel never saw it. Only `Environment.replay` catches it.
KVICTIM = "_agstKernel.lean"
KVICTIM_SRC = """unsafe def agst_kfalse_unsafe_rec : False := agst_kfalse_unsafe_rec
set_option debug.skipKernelTC true in
theorem agst_kfalse : False := agst_kfalse_unsafe_rec
theorem agst_kheadline : (1 : Nat) = 2 := agst_kfalse.elim
"""

# M9: a shipping module hidden by a name PREFIX (the old exclusion) — must be audited.
CHEAT = "_agstCheat.lean"
CHEAT_SRC = "theorem agst_cheat_false : False := sorry\n"
# M9b: a shipping module hidden in a SUBDIRECTORY — must be discovered.
SUBCHEAT = "_agstsub/Cheat.lean"
SUBCHEAT_SRC = "theorem agst_sub_false : False := sorry\n"


def cmd_selftest(do_build=True):
    print("== whole-tree axiom gate — paired negative-control self-test ==")
    ok = True
    _cleanup_selftest()

    if do_build:
        b = subprocess.run(["lake", "build", "axiomgate"], cwd=VERIF,
                           capture_output=True, text=True)
        if b.returncode != 0:
            print("FATAL: could not build axiomgate:\n" + b.stdout + b.stderr)
            return 1

    mods = covered_modules()

    # ---- POSITIVE P0: clean tree PASSES, kernel replay ok, replay covers all -
    rc, out, err = run_gate(mods, do_build=False)
    modules, thms, checked, replay, replay_n = parse_gate_output(out)
    if (rc == 0 and "GATE PASSED" in out and replay == "ok"
            and replay_n == checked and not coverage_errors()):
        print(f"  [PASS] P0  clean tree -> gate GREEN (exit 0), KERNEL_REPLAY ok "
              f"(replayed {replay_n} == {checked} audited), coverage exact")
    else:
        ok = False
        print(f"  [FAIL] P0  clean tree not clean (rc={rc}, replay={replay}, "
              f"replay_n={replay_n} vs checked={checked}, cov_errs={coverage_errors()})\n{out}\n{err}")

    # ---- POSITIVE P1: completeness GREEN on clean tree --------------------
    p1_failures = completeness_check(modules, thms, verbose=False)
    if rc == 0 and not p1_failures:
        print(f"  [PASS] P1  completeness GREEN ({checked} decls, {len(thms)} theorems covered)")
    else:
        ok = False
        print(f"  [FAIL] P1  completeness on clean tree: {p1_failures}")

    # ---- P2: the completeness regexes are LIVE (NS1 — `_DECL_IN` not dead) --
    m_start = _DECL.search("@[simp] theorem p2_a : True := trivial")
    m_in = _DECL_IN.search("set_option debug.skipKernelTC true in theorem p2_b : True := trivial")
    m_open = _DECL_IN.search("open Nat in lemma p2_c : True := trivial")
    if (m_start and m_start.group(1) == "p2_a"
            and m_in and m_in.group(1) == "p2_b"
            and m_open and m_open.group(1) == "p2_c"):
        print("  [PASS] P2  completeness regexes match line-start AND `set_option/open … in theorem`")
    else:
        ok = False
        print(f"  [FAIL] P2  completeness regex gap (start={m_start}, in={m_in}, open={m_open})")

    # ---- NEGATIVE N1/N6: every hard bypass form + real unsafe -> RED (in the
    #      VIOLATION stream, not the CHECKED_THM stdout) ---------------------
    try:
        (VERIF / VICTIM).write_text(VICTIM_SRC, encoding="utf-8")
        compiled, clog = _compile_module(VICTIM)
        if not compiled:
            ok = False
            print(f"  [FAIL] N1  could not compile the victim module:\n{clog}")
        else:
            vmod = _module_name(VERIF / VICTIM, VERIF)
            rc, out, err = run_gate([vmod], do_build=False)
            # violations go to STDERR; match there so a token cannot be satisfied by
            # the gate's own CHECKED_THM stdout line (the S2 tautology fix).
            viol = "\n".join(l for l in err.splitlines() if l.strip().startswith("- "))
            expect_hard = {
                "plain axiom":       "agst_plain_axiom",
                "private axiom":     "agst_private_axiom",
                "@[simp] axiom":     "agst_simp_axiom",
                "sorry/sorryAx":     "sorryAx",
                "native_decide":     "native_decide",   # in the axiom NAME on a violation line
                "unsafe def":        "agst_unsafe_def",
                "forged _unsafe_rec name (M1)":  "agst_forged_unsafe_rec",
                "unsafe opaque (M1b)":           "agst_unsafe_opaque",
            }
            partial_reported = "PARTIAL" in out and "agst_partial_def" in out
            if rc == 0:
                ok = False
                print("  [FAIL] N1  victim module PASSED the gate (should be RED)")
            else:
                missing = [k for k, tok in expect_hard.items() if tok not in viol]
                if missing:
                    ok = False
                    print(f"  [FAIL] N1  gate RED but the VIOLATION stream did not flag: {missing}\n---VIOL---\n{viol}\n---")
                elif not partial_reported:
                    ok = False
                    print(f"  [FAIL] N1  gate did not INVENTORY the planted partial def\n{out}")
                else:
                    print(f"  [PASS] N1/N6  gate RED; violation stream flagged all {len(expect_hard)} hard forms "
                          f"(incl. forged `_unsafe_rec` name + unsafe opaque) and INVENTORIED the partial")
    finally:
        _cleanup_selftest()

    # ---- NEGATIVE N5: kernel-skipped `False` -> KERNEL_REPLAY FAILED -> RED
    try:
        (VERIF / KVICTIM).write_text(KVICTIM_SRC, encoding="utf-8")
        compiled, clog = _compile_module(KVICTIM)
        if not compiled:
            ok = False
            print(f"  [FAIL] N5  could not compile the skipKernelTC victim (unexpected):\n{clog}")
        else:
            kmod = _module_name(VERIF / KVICTIM, VERIF)
            rc, out, err = run_gate([kmod], do_build=False)
            blob = out + err
            # NS5: the replay failure must name the THEOREM (`agst_kfalse`), so the
            # control cannot be satisfied by a helper-only kernel rejection.
            replay_failed = "KERNEL REPLAY FAILED" in blob or "KERNEL_REPLAY FAILED" in blob
            # Quote-delimited AND matched on the VIOLATION stream only, so it matches
            # the THEOREM `'agst_kfalse'` in the replay error — not the helper
            # `'agst_kfalse_unsafe_rec'` (of which `agst_kfalse` is a prefix), and not
            # the gate's own `CHECKED_THM` stdout (the r1-S2 tautology class).
            names_theorem = "agst_kfalse'" in err
            if rc != 0 and replay_failed and names_theorem:
                print("  [PASS] N5  skipKernelTC `False` theorem -> KERNEL REPLAY FAILED "
                      "(replay names `agst_kfalse`) -> gate RED")
            else:
                ok = False
                print(f"  [FAIL] N5  kernel-unchecked `False` not caught as expected "
                      f"(rc={rc}, replay_failed={replay_failed}, names_theorem={names_theorem})\n{blob}")
    finally:
        _cleanup_selftest()

    # ---- NEGATIVE N7: a prefixed AND a subdir shipping module cannot hide ---
    #   N7a: `_agstCheat.lean` built -> discovery INCLUDES it -> gate RED (audited).
    #   N7b: `_agstsub/Cheat.lean` source (unbuilt) -> coverage_errors RED (discovered).
    try:
        (VERIF / CHEAT).write_text(CHEAT_SRC, encoding="utf-8")
        compiled, clog = _compile_module(CHEAT)
        cmod = _module_name(VERIF / CHEAT, VERIF)
        built_includes = cmod in discover_built_modules()
        rc, out, err = run_gate([cmod], do_build=False)
        n7a = compiled and built_includes and rc != 0 and "sorryAx" in (out + err)
        (VERIF / "_agstsub").mkdir(exist_ok=True)
        (VERIF / SUBCHEAT).write_text(SUBCHEAT_SRC, encoding="utf-8")
        subname = _module_name(VERIF / SUBCHEAT, VERIF)
        cov = coverage_errors()
        n7b = any(subname in c and "NO built .olean" in c for c in cov)
        if n7a and n7b:
            print(f"  [PASS] N7  prefixed built module `{cmod}` is discovered & RED; "
                  f"subdir source `{subname}` is discovered by coverage (no silent exclusion)")
        else:
            ok = False
            print(f"  [FAIL] N7  coverage exclusion hole (n7a={n7a} built_includes={built_includes} rc={rc}; n7b={n7b})\ncov={cov}")
    finally:
        _cleanup_selftest()

    # ---- NEGATIVE N3/N4: completeness parser RED on a dropped thm/module,
    #      asserted GREEN-BEFORE, RED-AFTER (the S3 fix) ---------------------
    rc, clean_out, err = run_gate(mods, do_build=False)
    modules, thms, checked, replay, replay_n = parse_gate_output(clean_out)
    base_failures = completeness_check(modules, thms, verbose=False)
    src = source_theorems()
    some_thm = next((n for names in src.values() for n in names), None)
    if base_failures:
        ok = False
        print(f"  [FAIL] N3/N4  base tree is NOT green before doctoring: {base_failures[:3]}")
    elif some_thm:
        doctored = {t for t in thms if t.split(".")[-1] != some_thm}
        f3 = completeness_check(modules, doctored, verbose=False)
        if any(some_thm in x for x in f3):
            print(f"  [PASS] N3  green-before; dropping theorem `{some_thm}` -> completeness RED")
        else:
            ok = False
            print(f"  [FAIL] N3  dropping `{some_thm}` did not trip completeness")
        drop_mod = mods[0]
        f4 = completeness_check(modules - {drop_mod}, thms, verbose=False)
        if any(drop_mod in x and "did NOT report covering" in x for x in f4):
            print(f"  [PASS] N4  green-before; dropping module `{drop_mod}` -> completeness RED")
        else:
            ok = False
            print(f"  [FAIL] N4  dropping module `{drop_mod}` did not trip completeness")

    _cleanup_selftest()
    print("\nSELF-TEST " + ("PASSED — every gate is paired with a control that provably goes RED."
                            if ok else "FAILED — see [FAIL] lines above."))
    return 0 if ok else 1


def main(argv):
    if len(argv) < 2 or argv[1] not in ("gate", "completeness", "selftest"):
        print(__doc__)
        return 2
    do_build = "--no-build" not in argv
    if argv[1] == "gate":
        return cmd_gate(do_build=do_build)
    if argv[1] == "completeness":
        return cmd_completeness(do_build=do_build)
    return cmd_selftest(do_build=do_build)


if __name__ == "__main__":
    raise SystemExit(main(sys.argv))
