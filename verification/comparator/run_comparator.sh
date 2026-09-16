#!/usr/bin/env bash
# Comparator — independent-kernel re-check gate for the PythScribe verified core.
#
# Two layers, strongest-available first:
#
#   LAYER 1 (ALWAYS runs, no external deps): the axiom-footprint gates.
#     `lake build` + TWO gates:
#       (1a) `python comparator/whole_tree_axiom_gate.py gate` — the WHOLE-TREE
#            gate: re-imports EVERY shipping verification module into a fresh
#            env, RE-TYPECHECKS every decl through the kernel (Environment.replay),
#            and asserts, per declaration, axiom footprint ⊆
#            {propext, Classical.choice, Quot.sound}, no user-declared axiom, no
#            `unsafe`, and imports ⊆ Init∪covered (partial/implemented_by/extern
#            inventoried). Covers ALL verification/*.lean (not just
#            PythExpandVerify's import closure) and is immune to grep bypasses.
#       (1b) `python comparator/axiom_footprint.py gate` — the per-HEADLINE
#            subset gate + the formalization.yaml manifest binding (PythExpandVerify
#            headline claims + their refuter witnesses).
#     This is ~80% of the trust value and is what CI enforces.
#
#   LAYER 2 (runs iff lean4export + nanoda_bin are present): the true
#     Comparator — export the elaborated proof terms and RE-TYPE-CHECK them in
#     an INDEPENDENT Rust Lean-4 kernel (nanoda_bin), with permitted_axioms
#     pinned to the trio. Because our core is dependency-free the export is
#     small and this covers ALL declarations of PythExpandVerify's export closure
#     (the exported module) — the whole-tree axiom footprint over the sibling
#     proof modules is enforced by Layer 1a above, not by this export.
#
# Point both env vars at the built binaries to enable Layer 2:
#   LEAN4EXPORT_BIN=/path/to/lean4export/.lake/build/bin/lean4export
#   NANODA_BIN=/path/to/nanoda_lib/target/release/nanoda_bin
# (Build instructions in comparator.md. lean4export MUST be built against the
#  SAME toolchain as the core — leanprover/lean4:v4.31.0 — to read its .olean.)
#
# landrun (Landlock sandbox, ten-proofs) is Linux-only and NOT required for the
# re-check itself; skip on Windows/macOS.
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
VERIF="$(cd "$HERE/.." && pwd)"
cd "$VERIF"

echo "=================================================================="
echo "Comparator LAYER 1 — axiom-footprint gates (always on)"
echo "=================================================================="
lake build
echo "--- 1a: whole-tree gate (all verification/*.lean) ---"
python comparator/whole_tree_axiom_gate.py gate --no-build
echo "--- 1b: per-headline subset gate + formalization.yaml binding ---"
python comparator/axiom_footprint.py gate --no-build

echo
echo "=================================================================="
echo "Comparator LAYER 2 — independent-kernel re-check (lean4export+nanoda)"
echo "=================================================================="
LEAN4EXPORT_BIN="${LEAN4EXPORT_BIN:-}"
NANODA_BIN="${NANODA_BIN:-}"
if [[ -z "$LEAN4EXPORT_BIN" || -z "$NANODA_BIN" ]]; then
  echo "SKIP: set LEAN4EXPORT_BIN and NANODA_BIN to enable the independent re-check."
  echo "      Layer 1 already enforced the pinned footprint from the Lean kernel."
  exit 0
fi
if [[ ! -x "$LEAN4EXPORT_BIN" ]]; then echo "ERROR: LEAN4EXPORT_BIN not executable: $LEAN4EXPORT_BIN"; exit 3; fi
if [[ ! -x "$NANODA_BIN"     ]]; then echo "ERROR: NANODA_BIN not executable: $NANODA_BIN"; exit 3; fi

NDJSON="$HERE/pythexpand.ndjson"
echo "[1/2] exporting PythExpandVerify environment -> $NDJSON"
# lean4export must run inside the core's lake env so it sees the built .olean.
lake env "$LEAN4EXPORT_BIN" PythExpandVerify > "$NDJSON"
echo "      export size: $(wc -c < "$NDJSON") bytes, $(wc -l < "$NDJSON") lines"

echo "[2/2] independent re-check via nanoda_bin (permitted_axioms = the trio)"
# nanoda type-checks on its MAIN thread with deep recursion; raise the stack so
# the whole-core export does not overflow it (no-op / best-effort off Linux).
ulimit -s unlimited 2>/dev/null || true
# Point the committed config at the freshly written export.
CFG="$HERE/.nanoda-run.json"
python - "$HERE/nanoda-config.json" "$NDJSON" "$CFG" <<'PY'
import json, sys
cfg = json.load(open(sys.argv[1]))
cfg["export_file_path"] = sys.argv[2]
json.dump(cfg, open(sys.argv[3], "w"), indent=2)
PY
"$NANODA_BIN" "$CFG"
echo "OK: independent kernel accepted the export within the pinned axiom footprint."
