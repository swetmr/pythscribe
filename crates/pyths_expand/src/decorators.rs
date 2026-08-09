//! Decorator-alias table for Tier A `.psc` compression.
//!
//! A decorator alias is a single-letter shorthand that replaces a full
//! decorator name *when it occupies the decorator slot on its own line*
//! (with no call-args). For example `@c` → `@component`. Aliases with
//! call-args (`@d(coerce=True)`) are also supported.

pub struct DecoratorAlias {
    pub alias: &'static str,
    pub canonical: &'static str,
}

pub const ALIASES: &[DecoratorAlias] = &[
    DecoratorAlias {
        alias: "@c",
        canonical: "@component",
    },
    DecoratorAlias {
        alias: "@d",
        canonical: "@dataclass",
    },
    DecoratorAlias {
        alias: "@v",
        canonical: "@validator",
    },
    DecoratorAlias {
        alias: "@h",
        canonical: "@handler",
    },
    DecoratorAlias {
        alias: "@k",
        canonical: "@check",
    },
];

pub fn lookup(alias: &str) -> Option<&'static str> {
    ALIASES
        .iter()
        .find(|a| a.alias == alias)
        .map(|a| a.canonical)
}
