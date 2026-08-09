/-
  GENERATED FILE — do not edit by hand.

  The committed Tier-A tables, generated from
    crates/pyths_expand/src/presets.rs    :: PRESETS
    crates/pyths_expand/src/decorators.rs :: ALIASES
  by verification/gen-tiera-data.py. CI regenerates this file and
  fails on any diff, so the Lean Tier-A instantiation always
  quantifies over the SHIPPING tables.
-/

namespace PythExpandVerify

/-- The committed import-preset table (marker, canonicalImportLine) — 8 entries. -/
def committedPresets : List (String × String) := [
  ("R.", "from pyths.react import component, use_state"),
  ("R*", "from pyths.react import component, use_state, use_effect, use_callback, use_memo"),
  ("R+", "from pyths.react import component, use_state, use_effect, use_callback, use_memo, use_ref, use_context"),
  ("A*", "from pyths.asyncio import gather, sleep"),
  ("T*", "from dataclasses import dataclass"),
  ("T+", "from dataclasses import dataclass, Field"),
  ("D*", "from pyths.dom import query, query_all, get_element_by_id, set_text, get_text, add_event_listener"),
  ("W*", "from pyths.web import handler, Response")
]

/-- The committed decorator-alias table (alias, canonicalDecorator) — 5 entries. -/
def committedDecorators : List (String × String) := [
  ("@c", "@component"),
  ("@d", "@dataclass"),
  ("@v", "@validator"),
  ("@h", "@handler"),
  ("@k", "@check")
]

end PythExpandVerify
