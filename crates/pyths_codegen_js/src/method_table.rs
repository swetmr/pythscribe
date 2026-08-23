//! Source-of-truth catalog of Python collection/string methods.
//!
//! This file enumerates **every** method on `str` (47), `list` (11),
//! `dict` (11), `set` (17), and `tuple` (2). Each entry says how the
//! codegen lowers a call site; entries marked `Strategy::Unsupported`
//! are explicit "we don't support this yet — here's why" rows that
//! produce a compile error rather than silently emitting broken JS.
//!
//! Adding a new method means adding a row here. Removing a row is a
//! breaking change. The dispatcher in `method_lowering.rs` consumes
//! this table; do not maintain the lowering data anywhere else.
//!
//! Authority: https://docs.python.org/3/library/stdtypes.html
//!
//! ## Strategies
//!
//! - `Rename(js_name)` — same args, swap the attribute name.
//!   `xs.append(v)` → `xs.push(v)`
//! - `Inline(spec)` — small JS expression, defined in `InlineSpec` enum
//!   and emitted by `emit_inline_spec` in `emit.rs`.
//! - `Hybrid` — Inline for simple-Name receivers, Runtime for complex.
//! - `Runtime(helper)` — `pyHelper(receiver, ...args)` from pyths-runtime.
//! - `Unsupported(reason)` — compile-time diagnostic.

use crate::method_lowering::InlineSpec;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReceiverKind {
    /// Method only legal on string receivers.
    Str,
    /// Method only legal on list receivers.
    List,
    /// Method only legal on dict receivers (plain JS object backing).
    Dict,
    /// Method only legal on set receivers (JS Set).
    Set,
    /// Method only legal on tuple receivers (JS array — same shape as list).
    Tuple,
    /// Method legal on multiple receiver types — runtime helper sniffs
    /// the receiver shape (`Array.isArray`, `instanceof Set`, etc.) and
    /// dispatches accordingly.
    Multi,
    /// Method only legal on numeric receivers (int / float).
    Num,
}

#[derive(Debug, Clone, Copy)]
pub enum Strategy {
    Rename(&'static str),
    Inline(InlineSpec),
    Hybrid {
        inline: InlineSpec,
        runtime: &'static str,
    },
    Runtime(&'static str),
    /// Compile-time error. The string is shown to the user as the reason
    /// (with a "use X instead" hint where applicable).
    Unsupported(&'static str),
}

#[derive(Debug, Clone, Copy)]
pub struct Entry {
    pub name: &'static str,
    pub receiver: ReceiverKind,
    pub strategy: Strategy,
}

impl Entry {
    const fn r(name: &'static str, receiver: ReceiverKind, js: &'static str) -> Self {
        Self {
            name,
            receiver,
            strategy: Strategy::Rename(js),
        }
    }
    const fn i(name: &'static str, receiver: ReceiverKind, spec: InlineSpec) -> Self {
        Self {
            name,
            receiver,
            strategy: Strategy::Inline(spec),
        }
    }
    const fn h(
        name: &'static str,
        receiver: ReceiverKind,
        inline: InlineSpec,
        runtime: &'static str,
    ) -> Self {
        Self {
            name,
            receiver,
            strategy: Strategy::Hybrid { inline, runtime },
        }
    }
    const fn rt(name: &'static str, receiver: ReceiverKind, helper: &'static str) -> Self {
        Self {
            name,
            receiver,
            strategy: Strategy::Runtime(helper),
        }
    }
    const fn u(name: &'static str, receiver: ReceiverKind, reason: &'static str) -> Self {
        Self {
            name,
            receiver,
            strategy: Strategy::Unsupported(reason),
        }
    }
}

/// The authoritative table of Python method lowerings. Read by
/// `method_lowering::lookup`. Order is: strings → lists → dicts → sets
/// → tuples. Within each group, alphabetical by Python name.
pub static TABLE: &[Entry] = &[
    // ============================================================
    // str methods (47) — docs.python.org/3/library/stdtypes.html#string-methods
    // ============================================================
    Entry::h("capitalize", ReceiverKind::Str, InlineSpec::Capitalize, "pyStrCapitalize"),
    Entry::i("casefold", ReceiverKind::Str, InlineSpec::Casefold),
    Entry::rt("center", ReceiverKind::Str, "pyStrCenter"),
    Entry::rt("count", ReceiverKind::Multi, "pyCount"), // shared with list/tuple — runtime dispatches
    Entry::u("encode", ReceiverKind::Str, "encode() returns bytes; not yet supported. Use TextEncoder().encode(s) directly."),
    Entry::rt("endswith", ReceiverKind::Str, "pyStrEndswith"),
    Entry::rt("expandtabs", ReceiverKind::Str, "pyStrExpandtabs"),
    // #301: `.find` collides with Array.prototype.find(callback) — the old
    // Rename→indexOf silently broke JS array searches (`xs.find(lambda ...)`
    // → xs.indexOf(fn) → -1). pyFind: string receivers get full Python
    // str.find (start/end args now honored too); non-strings dispatch to
    // their own native .find.
    Entry::rt("find", ReceiverKind::Multi, "pyFind"),
    Entry::rt("format", ReceiverKind::Str, "pyStrFormat"),
    Entry::u("format_map", ReceiverKind::Str, "format_map() not yet supported. Use s.format(**d) instead."),
    Entry::rt("index", ReceiverKind::Multi, "pyIndex"), // shared with list/tuple
    Entry::i("isalnum", ReceiverKind::Str, InlineSpec::Isalnum),
    Entry::i("isalpha", ReceiverKind::Str, InlineSpec::Isalpha),
    Entry::i("isascii", ReceiverKind::Str, InlineSpec::Isascii),
    Entry::i("isdecimal", ReceiverKind::Str, InlineSpec::Isdigit), // ASCII-equiv to isdigit
    Entry::i("isdigit", ReceiverKind::Str, InlineSpec::Isdigit),
    Entry::rt("isidentifier", ReceiverKind::Str, "pyStrIsidentifier"),
    Entry::h("islower", ReceiverKind::Str, InlineSpec::Islower, "pyStrIslower"),
    Entry::i("isnumeric", ReceiverKind::Str, InlineSpec::Isdigit), // ASCII-equiv
    Entry::rt("isprintable", ReceiverKind::Str, "pyStrIsprintable"),
    Entry::i("isspace", ReceiverKind::Str, InlineSpec::Isspace),
    Entry::rt("istitle", ReceiverKind::Str, "pyStrIstitle"),
    Entry::h("isupper", ReceiverKind::Str, InlineSpec::Isupper, "pyStrIsupper"),
    Entry::rt("join", ReceiverKind::Str, "pyStrJoin"),
    Entry::rt("ljust", ReceiverKind::Str, "pyStrLjust"),
    Entry::r("lower", ReceiverKind::Str, "toLowerCase"),
    Entry::h("lstrip", ReceiverKind::Str, InlineSpec::Lstrip, "pyStrLstrip"),
    Entry::u("maketrans", ReceiverKind::Str, "maketrans() is a static method building translation tables. Use str.replaceAll() chains instead."),
    Entry::rt("partition", ReceiverKind::Str, "pyStrPartition"),
    Entry::i("removeprefix", ReceiverKind::Str, InlineSpec::Removeprefix),
    Entry::i("removesuffix", ReceiverKind::Str, InlineSpec::Removesuffix),
    // WB-18: route through the runtime smart dispatcher so a regex held in a
    // VARIABLE (invisible to the WB-10 syntactic arg check) is applied as a
    // regex, not mis-routed to Python str.replace. Plain-string args keep
    // Python replace-all semantics (pyStrReplaceSmart falls back to
    // pyStrReplace); inline regex/fn still short-circuits to a verbatim native
    // `.replace` via container_dispatch_prefers_verbatim before reaching here.
    Entry::rt("replace", ReceiverKind::Str, "pyStrReplaceSmart"),
    // Wave-19 verification fix: raw lastIndexOf returns UTF-16 code-unit
    // offsets; Python counts code points ('𝔸x𝔸x'.rfind('x') is 3, not 5).
    Entry::rt("rfind", ReceiverKind::Str, "pyStrRfind"),
    Entry::rt("rindex", ReceiverKind::Str, "pyStrRindex"),
    Entry::rt("rjust", ReceiverKind::Str, "pyStrRjust"),
    Entry::rt("rpartition", ReceiverKind::Str, "pyStrRpartition"),
    Entry::rt("rsplit", ReceiverKind::Str, "pyStrRsplit"),
    Entry::h("rstrip", ReceiverKind::Str, InlineSpec::Rstrip, "pyStrRstrip"),
    Entry::rt("split", ReceiverKind::Str, "pyStrSplit"),
    Entry::rt("splitlines", ReceiverKind::Str, "pyStrSplitlines"),
    // Full CPython spec (tuple prefixes + start/end): runtime helper —
    // the bare .startsWith rename dropped every optional argument.
    Entry::rt("startswith", ReceiverKind::Str, "pyStrStartswith"),
    Entry::h("strip", ReceiverKind::Str, InlineSpec::Strip, "pyStrStrip"),
    Entry::rt("swapcase", ReceiverKind::Str, "pyStrSwapcase"),
    Entry::rt("title", ReceiverKind::Str, "pyStrTitle"),
    Entry::rt("translate", ReceiverKind::Str, "pyStrTranslate"),
    Entry::r("upper", ReceiverKind::Str, "toUpperCase"),
    Entry::i("zfill", ReceiverKind::Str, InlineSpec::Zfill),

    // ============================================================
    // list methods (11) — docs.python.org/3/tutorial/datastructures.html
    // ============================================================
    // #301: `.append` collides with DOM ParentNode.append (and any JS/user
    // object exposing append). Bare Rename→push miscompiled every non-list
    // receiver (el.append(node) → el.push(node) → TypeError). Hybrid: the
    // `.push` inline only fires for receivers PROVABLY list-typed (gated in
    // `hybrid_inline_applies`); everything else dispatches through pyAppend,
    // which keeps Python list semantics for arrays and calls the receiver's
    // own native append otherwise.
    Entry::h("append", ReceiverKind::List, InlineSpec::AppendList, "pyAppend"),
    Entry::h("clear", ReceiverKind::Multi, InlineSpec::ClearList, "pyClear"), // shared with dict/set
    Entry::h("copy", ReceiverKind::Multi, InlineSpec::CopyList, "pyCopy"),    // shared
    // count, index — shared with str above (Multi). Listed in str group.
    // #301: extend/insert — same receiver-collision discipline as `append`:
    // inline only on provably-list receivers, else runtime dispatch (native
    // method wins for non-list receivers that have one).
    Entry::h("extend", ReceiverKind::List, InlineSpec::ExtendList, "pyExtend"),
    Entry::h("insert", ReceiverKind::List, InlineSpec::InsertList, "pyInsert"),
    Entry::rt("pop", ReceiverKind::Multi, "pyPop"), // shared with dict/set
    Entry::rt("remove", ReceiverKind::Multi, "pyRemove"), // shared with set
    Entry::r("reverse", ReceiverKind::List, "reverse"),
    Entry::rt("sort", ReceiverKind::List, "pyListSort"),

    // ============================================================
    // generator methods (3) — round-4 pythonic sweep
    // docs.python.org/3/reference/expressions.html#generator-iterator-methods
    // JS generators speak a different protocol (`.next(v)` instead of
    // `.send(v)`, `{done}` instead of StopIteration) — runtime helpers
    // bridge it. Non-generator receivers with their own send/close/throw
    // (WebSocket, file-likes, user classes) dispatch straight to that
    // method, so JS interop is preserved.
    // ============================================================
    Entry::rt("close", ReceiverKind::Multi, "pyGenClose"),
    Entry::rt("send", ReceiverKind::Multi, "pyGenSend"),
    Entry::rt("throw", ReceiverKind::Multi, "pyGenThrow"),

    // ============================================================
    // dict methods (11) — docs.python.org/3/library/stdtypes.html#mapping-types-dict
    // ============================================================
    // clear, copy, pop — shared with list/set (above).
    // dict.fromkeys(it[, v]) / d.fromkeys(...) — CPython classmethod,
    // callable from the type or an instance; the receiver is ignored.
    Entry::rt("fromkeys", ReceiverKind::Dict, "pyDictFromkeys"),
    // autotester complex_numbers: x.conjugate() on ints/floats/complex.
    Entry::rt("conjugate", ReceiverKind::Multi, "pyConjugate"),
    // Always via the runtime helper (not a simple-receiver inline): `.get`
    // must dispatch on receiver shape so FormData / Map / URLSearchParams /
    // Headers (where `.get` is a real method) work as well as plain-object
    // dicts. Inlining `d[k]` was silently wrong for those (B-038 / #55).
    Entry::rt("get", ReceiverKind::Dict, "pyDictGet"),
    // keys/values/items were Object.keys/values/entries inlines until #83:
    // Map-backed dicts (non-string keys) have no own enumerable props, so
    // these now dispatch on receiver shape at runtime (plain object → the
    // Object.* form; Map/PyDict/FormData/URLSearchParams → real .keys()/
    // .values()/.entries()). pyDictItems additionally marks each pair as a
    // tuple so `repr(d.items())` shows `(k, v)` like CPython.
    Entry::rt("items", ReceiverKind::Dict, "pyDictItems"),
    Entry::rt("keys", ReceiverKind::Dict, "pyDictKeys"),
    Entry::rt("popitem", ReceiverKind::Dict, "pyDictPopitem"),
    Entry::rt("setdefault", ReceiverKind::Dict, "pyDictSetdefault"),
    // `update` exists on dict (merge other into d) AND set (union into s)
    // with different semantics. We can't tell them apart without type
    // info, so we dispatch through a runtime helper that sniffs the
    // receiver. Marked Multi so the duplicate-name check passes.
    Entry::rt("update", ReceiverKind::Multi, "pyUpdate"),
    Entry::rt("values", ReceiverKind::Dict, "pyDictValues"),

    // ============================================================
    // set methods (17) — docs.python.org/3/library/stdtypes.html#set-types-set-frozenset
    // ============================================================
    Entry::r("add", ReceiverKind::Set, "add"),
    // clear, copy, pop, remove — shared above.
    Entry::rt("difference", ReceiverKind::Set, "pySetDifference"),
    Entry::rt("difference_update", ReceiverKind::Set, "pySetDifferenceUpdate"),
    // #301: runtime-dispatched so a non-Set receiver with its own .discard
    // (user classes) isn't silently sent to a nonexistent .delete.
    Entry::rt("discard", ReceiverKind::Set, "pyDiscard"),
    Entry::rt("intersection", ReceiverKind::Set, "pySetIntersection"),
    Entry::rt("intersection_update", ReceiverKind::Set, "pySetIntersectionUpdate"),
    Entry::rt("isdisjoint", ReceiverKind::Set, "pySetIsdisjoint"),
    Entry::rt("issubset", ReceiverKind::Set, "pySetIssubset"),
    Entry::rt("issuperset", ReceiverKind::Set, "pySetIssuperset"),
    Entry::rt("symmetric_difference", ReceiverKind::Set, "pySetSymmetricDifference"),
    Entry::rt("symmetric_difference_update", ReceiverKind::Set, "pySetSymmetricDifferenceUpdate"),
    Entry::rt("union", ReceiverKind::Set, "pySetUnion"),
    // `update` is consolidated above with ReceiverKind::Multi — runtime
    // helper pyUpdate sniffs the receiver to pick dict/set behavior.

    // ============================================================
    // tuple methods (2) — same backing as list, count/index already
    // covered as Multi above. No tuple-only entries.
    // ============================================================

    // ============================================================
    // numeric methods (int / float)
    // ============================================================
    // float.is_integer() / int.is_integer() (3.12+) → whether the value
    // has no fractional part. Number()-coerces so bigint-backed ints
    // (always integral) report True.
    Entry::i("is_integer", ReceiverKind::Num, InlineSpec::IsInteger),
];

/// Valid POSITIONAL-argument arity for the Python container method of a
/// given name, plus the container receiver kind(s) the method belongs to.
///
/// This is the **source of truth** for the receiver-context + arity
/// dispatch in `emit.rs` (the F2 root fix). A method call whose positional
/// arity is impossible for the Python container method CANNOT be that
/// method — so codegen emits it verbatim (preserving every argument)
/// instead of the container lowering, which would silently drop or corrupt
/// arguments (`FormData().append("k", v)` lowered as 1-arg `list.append`
/// dropped `v`). One uniform table closes the whole collision class rather
/// than patching each method.
///
/// Only methods with a **well-defined positional arity** are listed.
/// Variadic set operations (`update`, `union`, `intersection`,
/// `difference`, `symmetric_difference`, their `_update` variants,
/// `isdisjoint`/`issubset`/`issuperset`) take `*others` and thus cannot be
/// arity-discriminated — they are intentionally omitted (documented
/// residual; the receiver-shape runtime helper still dispatches them).
#[derive(Debug, Clone, Copy)]
pub struct ContainerArity {
    /// Minimum positional args (excluding the receiver).
    pub min: usize,
    /// Inclusive maximum positional args; `None` = unbounded.
    pub max: Option<usize>,
    /// Receiver container kinds for which this name IS the Python method.
    /// If the receiver is provably one of these, keep the container
    /// lowering regardless of the (still-valid) arity.
    pub containers: &'static [ReceiverKind],
}

impl ContainerArity {
    /// Is `argc` a legal positional-argument count for this Python method?
    pub fn accepts(&self, argc: usize) -> bool {
        argc >= self.min && self.max.is_none_or(|m| argc <= m)
    }
}

/// Positional-arity signature for a collision-prone container method, or
/// `None` if the name is not arity-gated (str-only methods, variadic set
/// ops, or names absent from the container catalog).
///
/// Authority for arities: docs.python.org/3/library/stdtypes.html and
/// docs.python.org/3/tutorial/datastructures.html.
pub fn container_method_arity(name: &str) -> Option<ContainerArity> {
    use ReceiverKind::{Dict, List, Set, Tuple};
    const fn ca(
        min: usize,
        max: Option<usize>,
        containers: &'static [ReceiverKind],
    ) -> ContainerArity {
        ContainerArity {
            min,
            max,
            containers,
        }
    }
    Some(match name {
        // --- list mutators (fixed arity; the F2 arg-drop class) ---
        "append" => ca(1, Some(1), &[List]),
        "extend" => ca(1, Some(1), &[List]),
        "insert" => ca(2, Some(2), &[List]),
        "reverse" => ca(0, Some(0), &[List]),
        // --- shared list/set mutator ---
        "remove" => ca(1, Some(1), &[List, Set]),
        // --- set mutators ---
        "add" => ca(1, Some(1), &[Set]),
        "discard" => ca(1, Some(1), &[Set]),
        // --- shared list/dict/set (0-arg) ---
        "clear" => ca(0, Some(0), &[List, Dict, Set]),
        "copy" => ca(0, Some(0), &[List, Dict, Set]),
        // --- pop: list[i]? 0-1, dict(k[,d]) 1-2, set() 0 → combined 0-2 ---
        "pop" => ca(0, Some(2), &[List, Dict, Set]),
        // --- dict (0-arg views / popitem) ---
        "keys" => ca(0, Some(0), &[Dict]),
        "values" => ca(0, Some(0), &[Dict]),
        "items" => ca(0, Some(0), &[Dict]),
        "popitem" => ca(0, Some(0), &[Dict]),
        // --- dict (k[, default]) ---
        "get" => ca(1, Some(2), &[Dict]),
        "setdefault" => ca(1, Some(2), &[Dict]),
        // --- count/index: list/tuple 1 / str 1-3 → combined 1-3 (str
        //     receiver is a scalar, never "provably container" here, so the
        //     wider str arity is what the range must admit) ---
        "count" => ca(1, Some(3), &[List, Tuple]),
        "index" => ca(1, Some(3), &[List, Tuple]),
        _ => return None,
    })
}

/// Look up an entry by Python method name. O(N) linear scan over a small
/// static table — N ≈ 80, fast enough that hashing isn't worth the
/// dependency or constant-time overhead of a perfect hash.
///
/// Returns the *first* matching entry. Names that appear with multiple
/// receivers (e.g., `count` on str/list/tuple) are catalogued under
/// `ReceiverKind::Multi` with one row that delegates dispatch to a
/// runtime helper that sniffs the receiver shape at runtime. So the
/// "first match" lookup never gives the wrong answer.
pub fn lookup(name: &str) -> Option<&'static Entry> {
    TABLE.iter().find(|e| e.name == name)
}

/// Iterate the catalog. Used by tests + diagnostic tooling.
pub fn iter() -> impl Iterator<Item = &'static Entry> {
    TABLE.iter()
}
