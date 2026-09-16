#!/usr/bin/env python3
"""shadow_491 mutation ledger — the paired NEGATIVE controls for the #491 fix.

Each mutation disables ONE arm of the fix in the committed sources, rebuilds, runs the
3-way harness + the Rust controls that pin that arm, records which went RED, and
restores the file (`git checkout --`). A control that stays GREEN under the mutation
that removes its property would be vacuous — the ledger proves each one is load-bearing.

Run from the repo root on a CLEAN, COMMITTED tree (it uses `git checkout --` to
restore):  python tests/differential/shadow_491/mutations.py [--only M1,M4]
Writes tests/differential/shadow_491/evidence/mutation-ledger.txt.
"""
from __future__ import annotations

import argparse
import os
import re
import subprocess
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parent.parent.parent
LEDGER = HERE / "evidence" / "mutation-ledger.txt"

JS = "crates/pyths_codegen_js/src/emit.rs"
HIR = "crates/pyths_hir/src/wasm_analysis.rs"
WLIB = "crates/pyths_codegen_wasm/src/lib.rs"
WEMIT = "crates/pyths_codegen_wasm/src/emit.rs"
CGR = "crates/pyths_codegen_js/src/captured_global_rename.rs"

# (id, description, [(file, old, new)], expected-RED harness cases, expected-FAIL cargo tests)
MUTATIONS = [
    (
        "M1",
        "JS cell model OFF: `builtin_cell_init` returns None (no `export let N = <builtin>` cell; the base emit-order model)",
        [(JS,
          "    fn builtin_cell_init(&self, name: &str) -> Option<(String, Vec<&'static str>)> {\n",
          "    fn builtin_cell_init(&self, name: &str) -> Option<(String, Vec<&'static str>)> {\n        if !name.is_empty() {\n            return None;\n        }\n")],
        # 12_global_rebind moved to M5's coverage after the B1 mark_hoisted fix: an `Other` binder
        # (lambda) global rebind now routes through the S4 binder census, not the cell-init — M5 REDs it.
        ["02_after_def_call_time", "08_early_call_b3", "13_del_restores_builtin", "14_conditional_def", "18_late_lambda_len"],
        [("pyths_codegen_js", "behavioral_differential", "test_call_time_shadow_resolution_matches_cpython")],
    ),
    (
        "M2",
        "B3 early-call rule OFF: `early_reachable` returns the empty set (a WASM caller called before the shadowing def bakes the final binding)",
        [(HIR,
          "    let mut seen: HashSet<String> = HashSet::new();\n    for stmt in module.body.iter().take(before_idx.min(module.body.len())) {\n",
          "    let mut seen: HashSet<String> = HashSet::new();\n    if before_idx < usize::MAX {\n        return seen;\n    }\n    for stmt in module.body.iter().take(before_idx.min(module.body.len())) {\n")],
        ["08_early_call_b3", "19_multi_wasm_chain"],
        [("pyths_hir", "shadow_491", "b3_")],
    ),
    (
        "M3",
        "S1 local-shadow OFF at BOTH layers: admission `local_shadow_call` returns None AND the WASM emitter's ctx-local check is dropped",
        [(HIR,
          "    let locals = local_binding_names(params, body);\n    let mut called = Vec::new();\n    collect_called_functions(body, &mut called);\n    called.into_iter().find(|c| {\n        locals.contains(c)\n",
          "    let locals = local_binding_names(params, body);\n    let mut called = Vec::new();\n    collect_called_functions(body, &mut called);\n    called.into_iter().find(|c| {\n        false && locals.contains(c)\n"),
         (WEMIT,
          "            let user_shadow =\n                self.user_def_names.contains(name.as_str()) || ctx.get_local(name).is_some();\n",
          "            let user_shadow = self.user_def_names.contains(name.as_str());\n")],
        ["09_local_shadow"],
        [("pyths_hir", "shadow_491", "s1_")],
    ),
    (
        "M4",
        "twin construction defs-only (the pre-fix CLI construction): `twin_module` drops the module imports",
        [(WLIB,
          "            StmtKind::Import { .. } | StmtKind::ImportFrom { .. } => true,\n",
          "            StmtKind::Import { .. } | StmtKind::ImportFrom { .. } => false,\n")],
        [],
        [("pyths_codegen_wasm", "twin_consistency_491", "twin_module_is_imports"),
         ("pyths_cli", "shadow_491_cli", "twin_math_import_resolves_at_fallback")],
    ),
    (
        "M5",
        "S4 binder census OFF: `final_module_bindings` drops every `Other(_)` binder (only def / from-math-import are binders — the base authority)",
        [(HIR,
          "    for g in globals {\n        out.insert(\n            g,\n            (GLOBAL_WRITE_INDEX, ShadowBinding::Other(\"global write\")),\n        );\n    }\n    out\n}\n",
          "    for g in globals {\n        out.insert(\n            g,\n            (GLOBAL_WRITE_INDEX, ShadowBinding::Other(\"global write\")),\n        );\n    }\n    out.retain(|_, (_, b)| !matches!(b, ShadowBinding::Other(_)));\n    out\n}\n")],
        ["11a_assign_binder", "11b_lambda_binder", "11c_import_as_binder", "11d_class_binder", "11e_for_target_binder", "12_global_rebind"],
        [("pyths_hir", "shadow_491", "s4_")],
    ),
    (
        "M6",
        "S3 per-index wasm_skip OFF: every same-named def is skipped (the base name-keyed `wasm_skip`)",
        [(JS,
          "                    && self.wasm_skip_indices.get(name) == self.top_stmt_index.as_ref()\n",
          "                    && self.wasm_skip_indices.contains_key(name)\n")],
        ["10_import_def_import"],
        [("pyths_cli", "shadow_491_cli", "import_def_import_chains_load_and_resolve_last_wins")],
    ),
    (
        "M7",
        "class-cell call-time dispatch OFF: `emit_cell_class_call` never fires (a cell name that is a class is `new`-called statically)",
        [(JS,
          "        if optional || !self.module_cells.contains_key(name) {\n            return false;\n        }\n        let js = Self::sanitize_ident(name).into_owned();\n        self.need_runtime(\"__pyCall\");\n",
          "        if optional || !self.module_cells.contains_key(name) || !name.is_empty() {\n            return false;\n        }\n        let js = Self::sanitize_ident(name).into_owned();\n        self.need_runtime(\"__pyCall\");\n")],
        ["11d_class_binder"],
        [("pyths_codegen_js", "behavioral_differential", "test_call_time_shadow_resolution_matches_cpython")],
    ),
    (
        "M8",
        "hidden glue alias OFF: a cell name's glue export is imported plainly (`import { abs }` beside `export let abs` — the S3 double declaration, #499)",
        [(JS,
          "            if self.wasm_cell_names.contains(s.as_str()) {\n                hidden.push(format!(\"{} as __wasm${}\", js, js));\n",
          "            if self.wasm_cell_names.contains(s.as_str()) && s.is_empty() {\n                hidden.push(format!(\"{} as __wasm${}\", js, js));\n")],
        ["01_forward_ref", "06_demoted_caller", "07_twin_overflow", "16_math_alias_then_def"],
        [("pyths_codegen_js", "behavioral_differential", "test_wasm_routed_shadow_wins_in_demoted_js_caller")],
    ),
    (
        "M9",
        "method-scope #199 pre-declare OFF: the class-method emitter bypasses the shared `open_function_scope` prologue (push_scope + set_scope_globals only — the pre-fix method path), so `global N; N = v` in a method emits a method-local `let N`",
        [(JS,
          "        let param_names: Vec<String> = params.iter().map(|p| p.name.clone()).collect();\n        self.open_function_scope(body, &param_names);\n        // Declare every param EXCEPT the dropped receiver (bound as `this` /\n",
          "        let param_names: Vec<String> = params.iter().map(|p| p.name.clone()).collect();\n        self.push_scope(Self::collect_local_bindings(body, &param_names));\n        self.set_scope_globals(Self::collect_global_declared(body));\n        // Declare every param EXCEPT the dropped receiver (bound as `this` /\n")],
        ["21_method_global_write", "22_method_global_write_nonbuiltin", "23_static_classmethod_global_write"],
        [("pyths_codegen_js", "behavioral_differential", "test_method_global_write")],
    ),
    (
        "M10",
        "class-body global-write dispatch OFF: `is_class_body_global_write` never fires (the census still creates the cell, but the class-body installer emits `__pyClassAttr(C, \"N\", v)` again — the pre-fix class-body path — so `class C: global abs; abs = 7` leaves the module cell untouched)",
        [(JS,
          "        (compound || !bound.is_empty()) && bound.iter().all(|n| class_globals.contains(n))\n",
          "        false && (compound || !bound.is_empty()) && bound.iter().all(|n| class_globals.contains(n))\n")],
        ["24_classbody_global_write", "25_classbody_global_write_nonbuiltin", "26_classbody_global_write_compound", "27_classbody_nested_global_write"],
        [("pyths_codegen_js", "behavioral_differential", "test_class_body_global_write")],
    ),
    (
        "M11",
        "captured-`global` collision pre-pass OFF: `rename_captured_globals` returns None (the enclosing function's `let abs` captures the nested `global abs` access again — the pre-fix lexical capture; item-3 prints 7 / <pyAbs>)",
        [(CGR,
          "    if !has_global_inside_a_function(&module.body, false) {\n",
          "    if !has_global_inside_a_function(&module.body, false) || module.body.len() < usize::MAX {\n")],
        ["28_nested_global_captured_by_enclosing_local", "29_nested_global_param_two_level_nonlocal", "30_nested_global_class_comprehension_lambda"],
        [("pyths_codegen_js", "behavioral_differential", "test_nested_global_capture")],
    ),
    (
        "M12",
        "B1 ONE-authority OFF: `predeclare_outer_binding` declares but no longer marks a `global`/`nonlocal` name hoisted (the pre-fix prologue), so every arm keyed on `is_hoisted` — for-target (Name/tuple/range), match capture/star, except-as — writes a fresh block-local again",
        [(JS,
          "    fn predeclare_outer_binding(&mut self, name: &str) {\n        self.declare(name);\n        self.mark_hoisted(name);\n    }\n",
          "    fn predeclare_outer_binding(&mut self, name: &str) {\n        self.declare(name);\n    }\n")],
        ["32_function_global_for_target", "33_nonlocal_for_match_capture_classbody", "35_sf1_global_write_creates_module_binding"],
        [("pyths_codegen_js", "behavioral_differential", "test_function_scope_global_target_arms_match_cpython"),
         ("pyths_codegen_js", "behavioral_differential", "test_global_target_arms_emit_bare_outer_writes")],
    ),
    (
        "M15",
        "except-as `is_hoisted` consult OFF: the `as` alias is always a fresh catch-block `let N = __exc` again (the pre-fix arm; a `global N` alias is hoisted by `collect_hoisted_names` regardless of M12's prologue mark, so THIS is the load-bearing change for case 34 — a module reader saw the stale `int` during the handler)",
        [(JS,
          "                let outer_alias = handler\n                    .name\n                    .as_deref()\n                    .filter(|n| self.is_hoisted(n))\n",
          "                let outer_alias = handler\n                    .name\n                    .as_deref()\n                    .filter(|n| self.is_hoisted(n) && n.is_empty())\n")],
        ["34_except_as_global"],
        [("pyths_codegen_js", "behavioral_differential", "test_function_scope_global_target_arms_match_cpython"),
         ("pyths_codegen_js", "behavioral_differential", "test_global_target_arms_emit_bare_outer_writes")],
    ),
    (
        "M13",
        "SF1 create-the-global hoist OFF: the module-level `let N = __UNBOUND` for a binder-less `global N` write is never emitted (the pre-fix ReferenceError / fresh-local shape)",
        [(JS,
          "            for name in created {\n                self.need_runtime(\"__UNBOUND\");\n",
          "            for name in created.into_iter().filter(|_| false) {\n                self.need_runtime(\"__UNBOUND\");\n")],
        ["35_sf1_global_write_creates_module_binding"],
        [("pyths_codegen_js", "behavioral_differential", "test_function_scope_global_target_arms_match_cpython"),
         ("pyths_codegen_js", "behavioral_differential", "test_global_target_arms_emit_bare_outer_writes")],
    ),
    (
        "M14",
        "SF2 refusal OFF: the dotted-import head-collision scan never runs (the shape ships as the silent `let os = {}` capture again). NOTE: harness case 36 stays green under this mutation because `os/path` also fails to LINK under node (loud for an unrelated reason) — the load-bearing control is the Rust diagnostic test",
        [(CGR,
          "    if !deep.is_empty() {\n        collect_refused_dotted_imports(fname, body, &deep, &mut st.refused);\n    }\n",
          "    if deep.is_empty() {\n        collect_refused_dotted_imports(fname, body, &deep, &mut st.refused);\n    }\n")],
        [],
        [("pyths_codegen_js", "behavioral_differential", "test_dotted_import_head_collision_is_refused_loudly")],
    ),
]


def run(cmd, **kw):
    return subprocess.run(cmd, cwd=str(ROOT), capture_output=True, text=True, encoding="utf-8", errors="replace", **kw)


def apply(edits):
    for f, old, new in edits:
        p = ROOT / f
        s = p.read_text(encoding="utf-8")
        assert s.count(old) == 1, f"{f}: anchor not unique/found for mutation:\n{old}"
        p.write_text(s.replace(old, new), encoding="utf-8", newline="\n")


def restore(edits):
    files = sorted({f for f, _, _ in edits})
    r = run(["git", "checkout", "--", *files])
    assert r.returncode == 0, r.stderr


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--only")
    args = ap.parse_args()
    only = set(args.only.split(",")) if args.only else None
    if run(["git", "status", "--porcelain", JS, HIR, WLIB, WEMIT, CGR]).stdout.strip():
        print("refusing: the mutated files have uncommitted changes", file=sys.stderr)
        return 2
    LEDGER.parent.mkdir(parents=True, exist_ok=True)
    lines = [f"# #491 mutation ledger (each mutation applied to the committed sources, rebuilt, run, restored)\n# HEAD {run(['git','rev-parse','--short','HEAD']).stdout.strip()}\n"]
    all_ok = True
    for mid, desc, edits, red_cases, tests in MUTATIONS:
        if only and mid not in only:
            continue
        print(f"== {mid}: {desc}", flush=True)
        lines.append(f"\n## {mid} — {desc}\n")
        apply(edits)
        try:
            b = run(["cargo", "build", "-p", "pyths_cli"])
            if b.returncode != 0:
                lines.append(f"BUILD FAILED under mutation (a compile-time control):\n{b.stderr[-1500:]}\n")
                print("   build failed (counts as RED)")
                continue
            # harness
            h = run([sys.executable, str(HERE / "shadow_491.py")])
            red = set(re.findall(r"^RED\s+(\S+)", h.stdout, re.M))
            summary = [l for l in h.stdout.splitlines() if l.startswith("[shadow_491]")]
            lines.append(f"harness: {summary[-1] if summary else 'no summary'}")
            lines.append(f"  RED cases: {sorted(red) or '-'}")
            missing = [c for c in red_cases if c not in red]
            for c in red_cases:
                lines.append(f"  expected RED {c}: {'RED (ok)' if c in red else 'STILL GREEN <-- vacuous control'}")
            if missing:
                all_ok = False
            # cargo controls
            for crate, tfile, filt in tests:
                t = run(["cargo", "test", "-p", crate, "--test", tfile, filt])
                res = [l for l in t.stdout.splitlines() if l.startswith("test result")]
                failed = "FAILED" in (res[-1] if res else "") or t.returncode != 0
                lines.append(f"  cargo test -p {crate} --test {tfile} {filt}: {'FAILED (ok — control fires)' if failed else 'passed <-- vacuous control'}  [{res[-1] if res else t.stderr[-200:]}]")
                if not failed:
                    all_ok = False
            print("   " + "\n   ".join(lines[-(len(red_cases) + len(tests) + 2):]))
        finally:
            restore(edits)
    # rebuild the clean tree so the binary matches HEAD again
    run(["cargo", "build", "-p", "pyths_cli"])
    lines.append(f"\n# verdict: {'every mutation turned its paired control RED' if all_ok else 'SOME CONTROL STAYED GREEN — see above'}\n")
    LEDGER.write_text("\n".join(lines), encoding="utf-8", newline="\n")
    print(LEDGER.read_text(encoding="utf-8"))
    return 0 if all_ok else 1


if __name__ == "__main__":
    sys.exit(main())
