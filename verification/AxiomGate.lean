/-
  AxiomGate.lean — the WHOLE-TREE axiom-enforcement gate for the PythScribe verified core.

  WHY THIS EXISTS (the blocker it closes):
  The prior trust-base gate was a TWO-PART hole. (a) the CI step "no sorry / no
  extra axiom" was a *keyword grep* over the Lean source — bypassable by any
  form the regex does not spell (`private axiom`, an `@[simp]`/attribute-decorated
  `axiom`, a TAB-indented `axiom`, a term-level `sorryAx`, `native_decide`'s
  `ofReduceBool`/`ofReduceNat`, `unsafe`/`partial`). (b) the positive
  axiom-subset gate (comparator/axiom_footprint.py) + the nanoda re-check covered
  ONLY `PythExpandVerify`'s import closure — the sibling proof modules
  (Union7/Union8/C1C3C4Outcome/RoutingSoundness/RpcBoundary/C2TypeRepr/
  C8HostInterop/EffectOrder/InternalConsistency/JsWasmMirror + the generated
  *Data modules + Main) were NEVER axiom-audited as a whole tree.

  THE FIX — one authority over the WHOLE shipping tree, in the kernel, not in text:
  Given a set of module names (supplied as CLI args — the caller derives them
  from the filesystem so the coverage set can NEVER silently drift from the set
  of shipping `verification/*.lean` files), this program:
    1. `importModules` them into a FRESH environment (SafeVerify / lean4checker
       discipline — re-import, do not trust an incremental build's in-memory env),
    2. enumerates EVERY constant each module contributes (`ModuleData.constNames`
       — every `theorem`/`lemma`/`def`/`abbrev` + every kernel-generated
       auxiliary), and for each asserts, from the KERNEL:
         (A) axiom footprint ⊆ {propext, Classical.choice, Quot.sound}
             (`Lean.collectAxioms`) — this catches `sorryAx` (sorry), `ofReduceBool`
             / `ofReduceNat` (native_decide), and any smuggled axiom a decl DEPENDS
             on, regardless of how it was written;
         (B) NO decl is itself an `axiom` declared in a verification module — this
             catches an UNUSED `private axiom cheat : False`, a `@[simp] axiom`, a
             tab-indented `axiom`, etc. (the pinned trio live in Lean core `Init`,
             never in our modules, so any `axiomInfo` here is a violation);
         (C) NO decl is `unsafe` (`DefinitionVal.safety = .unsafe` /
             `ConstantInfo.isUnsafe`), keyed on SAFETY not on any name — the
             genuinely dangerous SafeVerify reject (`unsafeCast`). `partial` is
             benign INVENTORY (adds no axiom, opaque so unusable in a kernel-checked
             proof; native_decide reflection is already caught by (A); the tree uses
             it for recursive `Repr` display instances) — reported, never failed;
         (D) every covered module imports ONLY `Init.*` ∪ the covered set — no
             `import Lean`/external package (that would open `addDeclWithoutChecking`/
             `run_cmd`); and `@[implemented_by]`/`@[extern]` decls are inventoried
             (they swap COMPILED behaviour — a shipping-binding concern — though they
             do not touch the kernel-checked proof).
    3. RE-TYPECHECKS every covered declaration through the KERNEL via
       `Environment.replay` (lean4checker discipline): the elaborator's self-reported
       env is NOT the TCB, so a decl elaborated under `set_option debug.skipKernelTC
       true` (a `False` with an empty axiom footprint) or added via
       `addDeclWithoutChecking` is REJECTED by the kernel re-check. This is what makes
       "asserted from the kernel" literally true, not a dependency-walk claim.
    4. exits NONZERO on any violation; on success prints a machine-readable
       coverage report (per-module constant counts + every checked theorem name +
       the partial/companion/attr inventory + KERNEL_REPLAY status) that the Python
       driver cross-checks against the source (the completeness / "an unpinned
       shipping theorem is a build failure" meta-check).

  The axiom/kind checks read the ENVIRONMENT (not source text) so they are immune to
  every formatting bypass the old grep missed; the kernel replay closes the deeper
  "the elaborator wrote something the kernel never checked" hole.

  Run it via the Python driver (verification/comparator/whole_tree_axiom_gate.py),
  which discovers the module set and runs the paired negative-control self-test.
  Direct use:  lake env ./.lake/build/bin/axiomgate PythExpandVerify Union7 ...
-/
import Lean
open Lean

namespace AxiomGate

/-- The pinned axiom footprint (the ten-proofs trio). Our core is dependency-free
    (no Mathlib); this trio is the CEILING, inherited from Lean core `String`
    (where `Classical.choice`/`Quot.sound` enter). Many decls are strictly
    tighter (`[propext]`, `[propext, Quot.sound]`, or axiom-free). -/
def pinned : List Name := [``propext, ``Classical.choice, ``Quot.sound]

def pinnedSet : NameSet := pinned.foldl NameSet.insert NameSet.empty

def safetyStr : DefinitionSafety → String
  | .safe    => "safe"
  | .unsafe  => "unsafe"
  | .partial => "partial"

structure Report where
  violations   : Array String := #[]
  checkedDecls : Nat := 0
  checkedThms  : Array Name := #[]        -- theorem/lemma names (completeness backstop)
  perModule    : Array (Name × Nat) := #[]
  partials     : Array Name := #[]        -- user `partial def`s (opaque mains; benign inventory)
  companions   : Nat := 0                 -- compiler `_unsafe_rec` recursion companions (benign)
  attrHits     : Array String := #[]      -- @[implemented_by]/@[extern] decls (inventory)

/-- The whole-tree check over `mods`, in `CoreM` (which carries `MonadEnv`, all
    `Lean.collectAxioms` needs). A requested module absent from the imported
    environment is itself a violation (a coverage gap — the gate could not see
    that module's declarations). -/
def checkModules (mods : List Name) : CoreM Report := do
  let env ← getEnv
  let modNames := env.header.moduleNames
  let modData := env.header.moduleData
  let coveredSet : NameSet := mods.foldl NameSet.insert NameSet.empty
  let mut violations : Array String := #[]
  let mut checkedDecls : Nat := 0
  let mut checkedThms : Array Name := #[]
  let mut perModule : Array (Name × Nat) := #[]
  let mut partials : Array Name := #[]
  let mut companions : Nat := 0
  let mut attrHits : Array String := #[]
  let mut found : Array Name := #[]
  for i in [0:modNames.size] do
    let m := modNames[i]!
    if coveredSet.contains m then
      found := found.push m
      -- IMPORT RESTRICTION: a covered shipping-proof module may import ONLY `Init.*`
      -- (core) and other covered modules — no `import Lean`, no external package. This
      -- shuts the metaprogramming surface an unchecked decl would need
      -- (`import Lean` → `run_cmd … addDeclWithoutChecking`); with the kernel replay
      -- in `main` it is defense-in-depth, not the sole authority.
      for imp in modData[i]!.imports do
        unless imp.module.getRoot == `Init || coveredSet.contains imp.module do
          violations := violations.push
            s!"module `{m}` imports `{imp.module}` — outside (Init ∪ covered set); a shipping proof module must not pull a metaprogramming/trust surface"
      let cns := modData[i]!.constNames
      perModule := perModule.push (m, cns.size)
      for declName in cns do
        match env.find? declName with
        | none =>
          -- fail-closed: a constant the module lists but the env cannot resolve is
          -- a coverage gap, never a silent skip (NS2).
          violations := violations.push
            s!"{declName}: listed in module `{m}` constNames but `env.find?` returned none — unresolvable/coverage gap"
        | some ci =>
          if ci matches .thmInfo _ then
            checkedThms := checkedThms.push declName
          -- (B) no axiom may be DECLARED in a verification module (any axiomInfo here
          --     is a user axiom — the pinned trio lives in Lean core `Init`).
          if ci matches .axiomInfo _ then
            violations := violations.push
              s!"{declName}: AXIOM declared in verification module `{m}` — only the Lean-core pinned trio is permitted (no user axioms)"
          -- (C) `unsafe` is a HARD violation; `partial` is BENIGN inventory. The
          --     decision keys on SAFETY / kind, NEVER on a name suffix (a name is
          --     forgeable). A user `unsafe def`/`unsafe opaque`/unsafe constant is
          --     `.unsafe`/isUnsafe → rejected. A `.partial` DEFN is ALWAYS a
          --     compiler `_unsafe_rec` recursion companion (a user `partial def`'s
          --     MAIN is OPAQUE, not a defn) → counted (benign). A user `partial def`
          --     main is an opaque constant with a `_unsafe_rec` companion and
          --     isUnsafe=false → inventoried (benign: `partial` adds no axiom and is
          --     opaque, so it cannot be exploited in a kernel-checked proof; this
          --     tree uses it for recursive `Repr` display instances). `unsafe` is
          --     checked BEFORE the companion lookup so an `unsafe opaque` cannot be
          --     laundered as a "benign partial".
          match ci with
          | .defnInfo v =>
            match v.safety with
            | .unsafe  => violations := violations.push
                            s!"{declName}: unsafe def in `{m}` — unsafe is rejected"
            | .partial => companions := companions + 1
            | .safe    => pure ()
          | .opaqueInfo _ =>
            if ci.isUnsafe then
              violations := violations.push
                s!"{declName}: unsafe opaque in `{m}` — unsafe is rejected"
            else
              -- a user `partial def`'s opaque main has a `.partial` DEFN companion.
              -- Require the companion to actually be a `.partial` defn (NS4): a plain
              -- `opaque` whose user-declared `_unsafe_rec` sibling is a SAFE def is a
              -- plain opaque, not a partial, and must not be mislabeled.
              match env.find? (declName.str "_unsafe_rec") with
              | some (.defnInfo cv) => if cv.safety == DefinitionSafety.partial then
                                         partials := partials.push declName
              | _ => pure ()
          | _ =>
            if ci.isUnsafe then
              violations := violations.push s!"{declName}: unsafe constant in `{m}`"
          -- @[implemented_by] / @[extern] INVENTORY: they swap the COMPILED behaviour
          --   (decoupling the executable model from the theorem's definiendum — a
          --   shipping-binding concern), though they do NOT affect the kernel-checked
          --   proof. Reported so the header claim is truthful and any hit is auditable.
          if let some tgt := Compiler.implementedByAttr.getParam? env declName then
            attrHits := attrHits.push s!"@[implemented_by {tgt}] {declName} (module `{m}`)"
          if (externAttr.getParam? env declName).isSome then
            attrHits := attrHits.push s!"@[extern] {declName} (module `{m}`)"
          -- (A) axiom footprint ⊆ pinned (Lean.collectAxioms — the dependency closure)
          let axs ← collectAxioms declName
          for a in axs do
            unless pinnedSet.contains a do
              violations := violations.push
                s!"{declName}: depends on axiom `{a}` ∉ pinned footprint (module `{m}`)"
          checkedDecls := checkedDecls + 1
  for m in mods do
    unless found.contains m do
      violations := violations.push
        s!"MODULE NOT IMPORTED / not in environment: `{m}` — coverage gap, the gate cannot see its declarations"
  return { violations, checkedDecls, checkedThms, perModule, partials, companions, attrHits }

end AxiomGate

open AxiomGate in
/-- Entry point. Args = the module names to axiom-check (the whole shipping tree).
    The caller (whole_tree_axiom_gate.py) derives them from the filesystem so the
    covered set stays == the set of shipping `verification/*.lean` modules. -/
def main (args : List String) : IO UInt32 := do
  if args.isEmpty then
    IO.eprintln "usage: axiomgate <Module> [<Module> ...]   (whole-tree axiom footprint gate)"
    return 2
  initSearchPath (← findSysroot)
  let mods := args.map String.toName
  let imports := mods.map (fun m => ({ module := m } : Import))
  -- A requested module whose .olean is absent (e.g. a NEW shipping module that
  -- was not built) makes `importModules` throw. Catch it and report a clean
  -- coverage gap (nonzero) instead of crashing — an un-importable shipping
  -- module is exactly "an unpinned shipping theorem is a build failure".
  let env ← try
      importModules imports.toArray {} (trustLevel := 0)
    catch e =>
      IO.eprintln s!"GATE FAILED: MODULE NOT IMPORTED — coverage gap while importing {mods}: {e.toString}"
      IO.Process.exit 1
  let ctx : Core.Context := { fileName := "AxiomGate", fileMap := default }
  let stateCore : Core.State := { env := env }
  let (rep, _) ← (AxiomGate.checkModules mods).toIO ctx stateCore
  -- ── KERNEL REPLAY (lean4checker discipline; the AUTHORITY, not the elaborator) ──
  -- `importModules` + `collectAxioms` only walk the elaborator's SELF-REPORTED env;
  -- a decl elaborated under `set_option debug.skipKernelTC true` (or via
  -- `addDeclWithoutChecking`) would be trusted blindly. So RE-TYPECHECK every covered
  -- declaration through the kernel via `Environment.replay`: rebuild a fresh env from
  -- the covered modules' NON-covered imports (Init/core) and re-add every covered
  -- constant — the kernel then re-enforces type-correctness (rejecting a skipKernelTC
  -- `False`), the safe→unsafe barrier, and axiom declaration. Any rejection is a HARD
  -- violation. This makes the "asserted from the KERNEL" claim true.
  let coveredSet : NameSet := mods.foldl NameSet.insert NameSet.empty
  let mut newConstants : Std.HashMap Name ConstantInfo := {}
  let mut coreImports : Array Import := #[]
  let mut seenImp : NameSet := {}
  for i in [0:env.header.moduleNames.size] do
    let mn := env.header.moduleNames[i]!
    if coveredSet.contains mn then
      for nm in env.header.moduleData[i]!.constNames do
        if let some ci := env.find? nm then
          newConstants := newConstants.insert nm ci
      for imp in env.header.moduleData[i]!.imports do
        unless coveredSet.contains imp.module || seenImp.contains imp.module do
          seenImp := seenImp.insert imp.module
          coreImports := coreImports.push imp
  let replayErr : Option String ← try
      let base ← importModules coreImports {} (trustLevel := 0)
      let _ ← base.replay newConstants
      pure none
    catch e =>
      pure (some e.toString)
  let violations := match replayErr with
    | some msg => rep.violations.push
        s!"KERNEL REPLAY FAILED — a covered declaration does not re-typecheck in the kernel (skipKernelTC / addDeclWithoutChecking / unchecked import): {msg}"
    | none => rep.violations
  -- ── machine-readable report ──
  IO.println "== whole-tree axiom-footprint gate =="
  IO.println s!"pinned_footprint: {AxiomGate.pinned}"
  for (m, n) in rep.perModule do
    IO.println s!"MODULE {m} {n}"
  IO.println s!"CHECKED_DECLS {rep.checkedDecls}"
  for t in rep.checkedThms do
    IO.println s!"CHECKED_THM {t}"
  -- benign `partial def` inventory (opaque mains) + companion count — NOT failures.
  for p in rep.partials do
    IO.println s!"PARTIAL {p}"
  IO.println s!"PARTIAL_COUNT {rep.partials.size}"
  IO.println s!"COMPANION_COUNT {rep.companions}"
  for a in rep.attrHits do
    IO.println s!"ATTR {a}"
  IO.println s!"ATTR_COUNT {rep.attrHits.size}"
  let replayStatus := if replayErr.isNone then "ok" else "FAILED"
  -- REPLAY_CONSTANTS binds the positive arm (NS2): the driver asserts it equals
  -- CHECKED_DECLS, so a future regression that SHRINKS the set fed to the kernel
  -- re-check cannot still print `ok`. (Core `replay` internally skips isUnsafe/
  -- isPartial constants — those are already gate violations / benign opaques — but
  -- it IS fed every covered constant, which is what this count proves.)
  IO.println s!"REPLAY_CONSTANTS {newConstants.size}"
  IO.println s!"KERNEL_REPLAY {replayStatus}"
  if violations.isEmpty then
    IO.println s!"GATE PASSED: {rep.checkedDecls} decls across {mods.length} modules — kernel-replayed; every axiom footprint ⊆ pinned; no user-declared axioms; no unsafe; imports ⊆ Init∪covered. ({rep.partials.size} benign partial + {rep.companions} recursion companions + {rep.attrHits.size} implemented_by/extern inventoried.)"
    return 0
  else
    IO.eprintln s!"GATE FAILED: {violations.size} violation(s):"
    for v in violations do
      IO.eprintln s!"  - {v}"
    return 1
