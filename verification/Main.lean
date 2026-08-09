/-
  expanddiff — two roles:
  1. (stdin→stdout) executable models of the CONCRETE tiers, one per flag —
     each the subject of an arm of the model-vs-implementation differential
     (diff_harness.py):
       (default)   Tier Dict   `$NAME` domain dictionary   (strings.rs)
       --kwargs    Tier B      kwarg-position aliases      (kwargs.rs)
       --hooks     hooks tier  hook-call shorthand         (hooks.rs)
       --idioms    Tier E      `%NAME` idiom fragments     (idioms.rs)
       --tiera     Tier A      presets + decorator aliases (presets/decorators.rs)
     Tier C (PSX tag-DSL) has NO arm: it is not instantiated in Lean — see
     the "TIER A / pipeline" section of PythExpandVerify.lean for why.
  2. --print-route-table / --check-route-table <path> — emit or verify
     the subscript-routing decision table (RouteModel §7.2) against the
     committed fixture, binding the Lean rules to cert.rs.
-/
import PythExpandVerify

/-- Run one of the executable tier models over all of stdin. -/
def runTier (f : String → String) : IO UInt32 := do
  let stdin ← IO.getStdin
  let input ← stdin.readToEnd
  IO.print (f input)
  return 0

def main (args : List String) : IO UInt32 := do
  match args with
  | ["--print-route-table"] =>
    IO.print PythExpandVerify.routeTable
    return 0
  | ["--check-route-table", path] =>
    let fixture ← IO.FS.readFile path
    -- Escape sequences, not raw bytes: a raw CR here was silently
    -- converted to LF at commit time (autocrlf), turning the intended
    -- CRLF normalization into a no-op.
    let normalized := (fixture.replace "\r\n" "\n")
    if normalized = PythExpandVerify.routeTable then
      IO.println "route table: Lean model == committed fixture"
      return 0
    else
      IO.eprintln "route table MISMATCH: the Lean RouteModel disagrees with verification/route-table.txt"
      IO.eprintln "regenerate with: lake exe expanddiff --print-route-table > verification/route-table.txt"
      IO.eprintln "then make cert.rs agree (cargo test route_table_matches_committed_fixture)"
      return 1
  | ["--print-wasm-admission-table"] =>
    IO.print PythExpandVerify.wasmAdmissionTable
    return 0
  | ["--check-wasm-admission-table", path] =>
    let fixture ← IO.FS.readFile path
    let normalized := (fixture.replace "\r\n" "\n")
    if normalized = PythExpandVerify.wasmAdmissionTable then
      IO.println "wasm-admission table: Lean model == committed fixture"
      return 0
    else
      IO.eprintln "wasm-admission table MISMATCH: the Lean WasmAdmission model disagrees with verification/wasm-admission-table.txt"
      IO.eprintln "regenerate with: lake exe expanddiff --print-wasm-admission-table > verification/wasm-admission-table.txt"
      IO.eprintln "then make cert.rs agree (cargo test wasm_admission_table_matches_committed_fixture)"
      return 1
  | ["--kwargs"] => runTier PythExpandVerify.tierB
  | ["--hooks"] => runTier PythExpandVerify.tierHooks
  | ["--idioms"] => runTier PythExpandVerify.tierE
  | ["--tiera"] => runTier PythExpandVerify.tierA
  | [] => runTier (PythExpandVerify.expandDictStr PythExpandVerify.committedDict)
  | _ =>
    IO.eprintln "usage: expanddiff [--kwargs | --hooks | --idioms | --tiera \
      | --print-route-table | --check-route-table <path> \
      | --print-wasm-admission-table | --check-wasm-admission-table <path>] \
      (default: stdin→stdout dict expansion)"
    return 2
