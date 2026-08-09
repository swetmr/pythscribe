//! Python-method to JS-method lowering — dispatcher.
//!
//! The data lives in `method_table.rs` (the source of truth for which
//! Python methods exist and how each is lowered). This module is the
//! thin layer that:
//!
//! 1. Looks up a method name in the table.
//! 2. Translates the table's `Strategy` enum into a `MethodLowering`
//!    that the codegen consumes.
//! 3. Provides `is_simple_receiver` for the dispatcher's safety check.
//!
//! Adding a new method = adding a row in `method_table.rs`. Do NOT add
//! lowering logic here directly — go through the table.

use crate::method_table::{lookup as table_lookup, Strategy};

#[derive(Debug, Clone, Copy)]
pub enum MethodLowering {
    Rename(&'static str),
    Inline(InlineSpec),
    Hybrid {
        inline: InlineSpec,
        runtime: &'static str,
    },
    Runtime {
        helper: &'static str,
        #[allow(dead_code)]
        receiver_first: bool,
    },
    /// The method is a known Python idiom that we don't lower yet. The
    /// codegen surfaces the reason as a diagnostic and emits a `throw`
    /// so the JS module is still parseable.
    Unsupported(&'static str),
}

#[derive(Debug, Clone, Copy)]
pub enum InlineSpec {
    // ---- list ----
    /// `xs.append(v)` → `xs.push(v)` — #301: only for receivers PROVABLY
    /// list-typed (gated in `hybrid_inline_applies`); unknown receivers
    /// dispatch through pyAppend so DOM/JS `.append` keeps working.
    AppendList,
    ExtendList,
    InsertList,
    CopyList,
    ClearList,
    CountList,
    PopList,
    // ---- string ----
    Strip,
    Lstrip,
    Rstrip,
    Zfill,
    Capitalize,
    /// `s.casefold()` → `s.toLowerCase()` (ASCII-equivalent; full Unicode
    /// casefold is a separate spec; we approximate).
    Casefold,
    Isdigit,
    Isalpha,
    Isalnum,
    /// `s.isascii()` → `[...s].every(c => c.charCodeAt(0) < 128)` —
    /// receiver once, but uses iterator. Simple receiver only.
    Isascii,
    Isspace,
    Islower,
    Isupper,
    /// `s.removeprefix(p)` → `(s.startsWith(p) ? s.slice(p.length) : s)`
    /// — receiver and arg referenced multiple times; simple receiver only,
    /// arg must also be simple. We accept simple receiver and trust the
    /// arg is cheap (it's almost always a string literal in practice).
    Removeprefix,
    /// `s.removesuffix(p)` → `(s.endsWith(p) ? s.slice(0, -p.length) : s)`.
    Removesuffix,
    // ---- dict ----
    // DictKeys/DictValues/DictItems removed in #83: dict keys()/values()/
    // items() lower to shape-dispatching runtime helpers (Map-backed dicts
    // have no own enumerable props for the Object.* inlines to see).
    DictUpdate,
    // ---- numeric ----
    /// `x.is_integer()` → `Number.isInteger(Number(x))` — receiver once.
    IsInteger,
}

impl InlineSpec {
    pub fn needs_simple_receiver(self) -> bool {
        matches!(
            self,
            InlineSpec::ClearList
                | InlineSpec::Capitalize
                | InlineSpec::Islower
                | InlineSpec::Isupper
                | InlineSpec::Isascii
                | InlineSpec::Removeprefix
                | InlineSpec::Removesuffix
        )
    }
}

/// Look up the lowering for a Python method name. Translates from the
/// table's `Strategy` (data) into `MethodLowering` (codegen consumes).
/// Returns `None` for names not in the table — those pass through as
/// user-defined method calls.
pub fn method_lowering(name: &str) -> Option<MethodLowering> {
    let entry = table_lookup(name)?;
    Some(match entry.strategy {
        Strategy::Rename(js) => MethodLowering::Rename(js),
        Strategy::Inline(spec) => MethodLowering::Inline(spec),
        Strategy::Hybrid { inline, runtime } => MethodLowering::Hybrid { inline, runtime },
        Strategy::Runtime(helper) => MethodLowering::Runtime {
            helper,
            receiver_first: false,
        },
        Strategy::Unsupported(reason) => MethodLowering::Unsupported(reason),
    })
}

/// True iff the receiver is a "simple" expression — one that is safe to
/// reference more than once because it has no side effects and is cheap
/// to re-evaluate. Covers Name, StringLiteral, IntLiteral, FloatLiteral,
/// BoolLiteral, NoneLiteral. Hybrid lowerings only emit Inline form
/// when this holds; Inline-only specs that need it (per
/// `InlineSpec::needs_simple_receiver`) fall through to the verbatim
/// path when the receiver is complex.
pub fn is_simple_receiver(expr: &pyths_syntax::ast::Expr) -> bool {
    use pyths_syntax::ast::ExprKind as EK;
    matches!(
        &expr.kind,
        EK::Name(_)
            | EK::StringLiteral(_)
            | EK::IntLiteral(_)
            | EK::FloatLiteral(_)
            | EK::ImagLiteral(_)
            | EK::BoolLiteral(_)
            | EK::NoneLiteral
    )
}
