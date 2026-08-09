# HelloCard - the tri-track scaffold proof component.
#
# Dual-track with HelloCard.tsx (React oracle). Renders identical DOM for
# identical props (see HelloCard.test.tsx render-parity suite). Ships NO
# clone content by itself; this is the trivial component the scaffold
# proves the shared/vite/next/e2e harness against before any real clone
# (youtube/, netflix/, ...) is built on top of it.
#
# "use client" - interactive (use_state), consumed as a precompiled
# `HelloCard.client.js` island from the Next app's server pages; loaded
# live (no precompile step) by Vite via vite-plugin-pyths.
#
# A2 dogfood: the side-effect CSS import below is the real production use
# of `import "<path>"` inside the clone-demos workspace - see
# HelloCard.css and CONTRIBUTING.md "Styling".
#
# NOTE: this is a `#` comment block, not a triple-quoted module docstring,
# specifically to avoid a Next.js 16 Turbopack panic (Rust `hstr` crate,
# "do not lie on character boundary") triggered when a leading top-level
# *string-literal statement* (i.e. a real docstring) contains a non-ASCII
# multi-byte UTF-8 character within roughly its first ~16 bytes.
# Turbopack's client-directive prescan slices leading statement strings by
# byte offset without checking UTF-8 char boundaries. `#` comments never
# reach the compiled JS as a statement at all, so they sidestep the whole
# bug class. See CONTRIBUTING.md "Known friction" for the reproduction.
"use client"

import "./HelloCard.css"

from pyths.react import component, use_state


@c
def HelloCard(title, subtitle=None):
    liked, set_liked = us(False)
    return div(
        cn="hello-card",
        data_testid="hello-card",
        h2(data_testid="hello-title", title),
        subtitle and p(data_testid="hello-subtitle", subtitle),
        button(
            cn="hello-like-btn",
            data_testid="hello-like",
            data_liked=liked,
            oc=lambda: set_liked(not liked),
            "♥ Liked" if liked else "♡ Like",
        ),
    )
