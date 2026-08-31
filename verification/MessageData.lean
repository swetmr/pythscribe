/-
  GENERATED FILE — do not edit by hand.

  The canonical exception-message layer, generated from
  verification/message-table.json by verification/gen-message-data.py.
  CI regenerates this file and fails on any diff, so the C1C3C4
  message model always states the SAME literals the shipped-binding
  differential (message_shipped_binding.py) pins against the real
  `pyths` binary and the CPython oracle (cpython-3.14.7).
-/

namespace MessageData

/-- `zero_div`: "division by zero" -/
def zeroDiv : String := "division by zero"

/-- `zero_neg_pow`: "zero to a negative power" -/
def zeroNegPow : String := "zero to a negative power"

/-- `not_callable`: "'{t}' object is not callable" -/
def notCallable (t : String) : String :=
  "'" ++ t ++ "' object is not callable"

/-- `not_iterable`: "'{t}' object is not iterable" -/
def notIterable (t : String) : String :=
  "'" ++ t ++ "' object is not iterable"

/-- `not_container`: "argument of type '{t}' is not a container or iterable" -/
def notContainer (t : String) : String :=
  "argument of type '" ++ t ++ "' is not a container or iterable"

/-- `no_attribute`: "'{t}' object has no attribute '{a}'" -/
def noAttribute (t : String) (a : String) : String :=
  "'" ++ t ++ "' object has no attribute '" ++ a ++ "'"

/-- `not_subscriptable`: "'{t}' object is not subscriptable" -/
def notSubscriptable (t : String) : String :=
  "'" ++ t ++ "' object is not subscriptable"

/-- `list_index_oor`: "list index out of range" -/
def listIndexOor : String := "list index out of range"

/-- `str_index_oor`: "string index out of range" -/
def strIndexOor : String := "string index out of range"

/-- `key_error`: "'{k}'" -/
def keyError (k : String) : String :=
  "'" ++ k ++ "'"

/-- `overflow_index`: "cannot fit 'int' into an index-sized integer" -/
def overflowIndex : String := "cannot fit 'int' into an index-sized integer"

/-- `unsupported_operand`: "unsupported operand type(s) for {op}: '{a}' and '{b}'" -/
def unsupportedOperand (op : String) (a : String) (b : String) : String :=
  "unsupported operand type(s) for " ++ op ++ ": '" ++ a ++ "' and '" ++ b ++ "'"

end MessageData
