/-
  GENERATED FILE — do not edit by hand.

  The committed Tier-B kwarg-alias table, generated from
  crates/pyths_expand/src/kwargs.rs :: ALIASES by
  verification/gen-kwarg-data.py. CI regenerates this file and
  fails on any diff, so the Lean Tier-B instantiation always
  quantifies over the SHIPPING table.
-/

namespace PythExpandVerify

/-- The committed kwarg-alias table (alias, canonicalKwarg) — 9 entries. -/
def committedKwargs : List (String × String) := [
  ("st", "style"),
  ("cn", "class_name"),
  ("cl", "className"),
  ("oc", "on_click"),
  ("oh", "on_change"),
  ("os", "on_submit"),
  ("oa", "on_acknowledge"),
  ("ph", "placeholder"),
  ("dis", "disabled")
]

end PythExpandVerify
