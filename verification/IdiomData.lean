/-
  GENERATED FILE — do not edit by hand.

  The committed Tier-E idiom fixture, generated from
  verification/idiom-table.toml by verification/gen-idiom-data.py.
  CI regenerates this file and fails on any diff.

  NOTE (honest scoping): Tier E has no compiler-side table — the
  `%NAME` map is supplied by the user's pyths.toml and is empty by
  default. This fixture is therefore a TEST TABLE, not a shipped
  one; what the Tier-E proofs and the differential pin is the
  SCANNER (idioms.rs::substitute_with_map), which is shipped. The
  Lean theorems are stated for an ARBITRARY table; this fixture is
  what the differential harness feeds to both sides.
-/

namespace PythExpandVerify

/-- The committed Tier-E idiom fixture (name, canonicalFragment) — 9 entries. -/
def committedIdioms : List (String × String) := [
  ("10", "TEN_INTERCEPTED"),
  ("EMPTY", ""),
  ("GUARD", "if value is None:\n    return None"),
  ("HTTPCHECK", "if not response.ok:\n    raise Exception(\"http error\")\nreturn await response.json()"),
  ("LOG", "print(\"[debug] tier-e\")"),
  ("MODEXPR", "rem = total % 7"),
  ("PASS", "pass"),
  ("X2", "doubled = n * 2"),
  ("_priv", "internal = True")
]

end PythExpandVerify
