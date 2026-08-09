/-
  GENERATED FILE — do not edit by hand.

  The committed hook-alias table, generated from
  crates/pyths_expand/src/hooks.rs :: ALIASES by
  verification/gen-hook-data.py. CI regenerates this file and
  fails on any diff, so the Lean hooks-tier instantiation always
  quantifies over the SHIPPING table.
-/

namespace PythExpandVerify

/-- The committed hook-alias table (alias, canonicalHook) — 6 entries. -/
def committedHooks : List (String × String) := [
  ("us", "use_state"),
  ("ue", "use_effect"),
  ("um", "use_memo"),
  ("uc", "use_callback"),
  ("ur", "use_ref"),
  ("ux", "use_context")
]

end PythExpandVerify
