"""Client counter (compressed .psc), rendered by the server page.

Pre-compiled to Counter.client.js by the `precompile-client` npm step
before `next build`/`dev`. PythScribe client components (`"use client"`)
are pre-compiled to a `.client.js` sibling rather than transformed by the
Turbopack loader: Turbopack's client-reference proxy appends the loader's
`as` target extension to the *full* custom-extension filename (so
`Counter.psc` becomes an unresolvable `Counter.psc.js` id). The loader's
importer-aware rewrite resolves `./Counter` to the precompiled
`Counter.client.js` ahead of `Counter.psc`, keeping this island off the
loader path (see README). Server components compile via the loader."""
"use client"

R*


@c
def Counter(start=0):
    count, sc = us(start)

    return div(style={"padding": $p4})(
        p(style={"font_size": "32px"})(f"Count: {count}"),
        button(cn="primary", oc=lambda: sc(count + 1))("Increment"),
    )
