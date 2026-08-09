//! Preset-import macro table.
//!
//! A preset is a single sentinel token that occupies an entire line in `.psc`
//! and expands to a canonical PythScribe import line. Presets are the largest
//! single token-saving lever in Phase 2: a five-name React import is ~18
//! cl100k_base tokens; the `R*` sentinel is 1–2.

pub struct Preset {
    pub marker: &'static str,
    pub expansion: &'static str,
}

pub const PRESETS: &[Preset] = &[
    // Minimal React preset: the two names every stateful component uses.
    // `R*`'s five-name import drags three unused names into most expansions
    // (the generation-eval exhibits show the typical component needs exactly
    // component + use_state); `R.` keeps the canonical .ps import exact.
    Preset {
        marker: "R.",
        expansion: "from pyths.react import component, use_state",
    },
    Preset {
        marker: "R*",
        expansion: "from pyths.react import component, use_state, use_effect, use_callback, use_memo",
    },
    Preset {
        marker: "R+",
        expansion: "from pyths.react import component, use_state, use_effect, use_callback, use_memo, use_ref, use_context",
    },
    Preset {
        marker: "A*",
        expansion: "from pyths.asyncio import gather, sleep",
    },
    Preset {
        marker: "T*",
        expansion: "from dataclasses import dataclass",
    },
    Preset {
        marker: "T+",
        expansion: "from dataclasses import dataclass, Field",
    },
    Preset {
        marker: "D*",
        expansion: "from pyths.dom import query, query_all, get_element_by_id, set_text, get_text, add_event_listener",
    },
    Preset {
        marker: "W*",
        expansion: "from pyths.web import handler, Response",
    },
];

pub fn lookup(marker: &str) -> Option<&'static str> {
    PRESETS
        .iter()
        .find(|p| p.marker == marker)
        .map(|p| p.expansion)
}
