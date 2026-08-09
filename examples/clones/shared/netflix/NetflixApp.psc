# Netflix clone - PythScribe canonical track. Dual-track with NetflixApp.tsx
# (the React oracle); all three tracks (.tsx/.ps/.psc) must render identical
# DOM for the same props (see the *.test.tsx parity suites in this dir).
#
# Deliberately a SINGLE module: in the Next shell, "use client" .ps islands
# are precompiled to sibling .client.js files whose extensionless relative
# imports are NOT loader-rewritten, so a cross-file .ps -> .ps import inside
# the client graph would resolve to the .tsx oracle instead (resolveExtensions
# puts .tsx first). One island file per clone sidesteps that entirely.
#
# PythScribe features exercised here (the stress-test payload):
# - create_portal from react_dom (snake_case import - the fresh A1 fix)
# - create_context + use_context via a Provider alias (see WORKAROUND below)
# - use_effect with cleanup (Escape-to-close keydown listener + body scroll
#   lock/restore on the portal modal)
# - use_ref + imperative el.scrollBy for the carousel scroll buttons
# - list comprehensions over fixture data; f-strings for testids/gradients
#
# WORKAROUND (PythScribe PSX limitation, documented in the build report):
# `MyListContext.Provider(value=v, children)` compiles to a plain function
# CALL (member-expression callees are not treated as PSX component elements),
# which would throw at runtime. Aliasing the member to a Capitalized name
# first (`ListProvider = MyListContext.Provider`) makes the call site a
# recognized component element and emits createElement(ListProvider, ...).
#
# NOTE: `#` comment block, not a module docstring - Turbopack panics on
# non-ASCII bytes early in a leading docstring statement (see CONTRIBUTING.md
# "Known friction").
"use client"

import "./Netflix.css"

from pyths.react import component, use_state, use_effect, use_ref, use_context, create_context
from react_dom import create_portal

MyListContext = create_context(None)
ListProvider = MyListContext.Provider


def use_my_list():
    return ux(MyListContext)


@c
def MyListProvider(children=None):
    ids, set_ids = us([])

    def toggle(tid):
        if tid in ids:
            set_ids([i for i in ids if i != tid])
        else:
            set_ids(ids + [tid])

    value = {"ids": ids, "toggle": toggle}
    return ListProvider(value=value, children)


@c
def Hero(title, on_more_info):
    a0 = title["art"][0]
    a1 = title["art"][1]
    poster = title["poster"]
    hero_bg = (
        f"linear-gradient(90deg, rgba(20,20,20,0.9) 0%, rgba(20,20,20,0.45) 55%, rgba(20,20,20,0.1) 100%), url({poster})"
        if poster
        else f"linear-gradient(160deg, {a0} 0%, {a1} 100%)"
    )
    return header(
        cn="nf-hero",
        data_testid="nf-hero",
        st={
            "backgroundImage": hero_bg,
            "backgroundColor": a0,
            "backgroundSize": "cover",
            "backgroundPosition": "center",
        },
        div(
            cn="nf-hero-content",
            p(cn="nf-hero-kicker", "N SERIES"),
            h1(cn="nf-hero-title", data_testid="nf-hero-title", title["name"]),
            p(
                cn="nf-meta",
                span(cn="nf-match", f"{title['match']}% Match"),
                span(title["year"]),
                span(cn="nf-maturity", title["maturity"]),
                span(title["duration"]),
            ),
            p(cn="nf-hero-desc", data_testid="nf-hero-desc", title["description"]),
            div(
                cn="nf-hero-ctas",
                button(cn="nf-btn nf-btn-play", data_testid="nf-hero-play", "▶ Play"),
                button(
                    cn="nf-btn nf-btn-info",
                    data_testid="nf-hero-info",
                    oc=lambda: on_more_info(title),
                    "More Info",
                ),
            ),
        ),
    )


@c
def TitleCard(row_id, title, on_open):
    ctx = use_my_list()
    tid = title["id"]
    name = title["name"]
    in_list = tid in ctx["ids"]
    a0 = title["art"][0]
    a1 = title["art"][1]
    poster = title["poster"]
    card_bg = f"url({poster})" if poster else f"linear-gradient(135deg, {a0}, {a1})"

    def handle_toggle(e):
        e.stopPropagation()
        ctx["toggle"](tid)

    return div(
        cn="nf-card",
        data_testid=f"nf-card-{row_id}-{tid}",
        data_in_list=in_list,
        oc=lambda: on_open(title),
        div(
            cn="nf-card-art",
            st={
                "backgroundImage": card_bg,
                "backgroundColor": a0,
                "backgroundSize": "cover",
                "backgroundPosition": "center",
            },
            span(cn="nf-card-name", name),
        ),
        div(
            cn="nf-card-preview",
            data_testid=f"nf-preview-{row_id}-{tid}",
            span(cn="nf-match", f"{title['match']}% Match"),
            span(cn="nf-card-meta", f"{title['year']} | {title['duration']}"),
            button(
                cn="nf-card-toggle",
                data_testid=f"nf-toggle-{row_id}-{tid}",
                aria_label=(
                    f"Remove {name} from My List" if in_list else f"Add {name} to My List"
                ),
                oc=handle_toggle,
                "✓" if in_list else "+",
            ),
        ),
    )


@c
def Carousel(row_id, label, titles, on_open):
    track_ref = ur(None)

    def scroll(direction):
        el = track_ref.current
        if el:
            el.scrollBy({"left": direction * 600, "behavior": "smooth"})

    return section(
        cn="nf-row",
        data_testid=f"nf-row-{row_id}",
        h2(cn="nf-row-label", label),
        div(
            cn="nf-row-body",
            button(
                cn="nf-scroll nf-scroll-left",
                data_testid=f"nf-scroll-left-{row_id}",
                aria_label=f"Scroll {label} left",
                oc=lambda: scroll(-1),
                "‹",
            ),
            div(
                cn="nf-track",
                data_testid=f"nf-track-{row_id}",
                ref=track_ref,
                *[TitleCard(key=t["id"], row_id=row_id, title=t, on_open=on_open) for t in titles],
            ),
            button(
                cn="nf-scroll nf-scroll-right",
                data_testid=f"nf-scroll-right-{row_id}",
                aria_label=f"Scroll {label} right",
                oc=lambda: scroll(1),
                "›",
            ),
        ),
    )


@c
def DetailModal(title, on_close):
    ctx = use_my_list()
    tid = title["id"]
    in_list = tid in ctx["ids"]
    a0 = title["art"][0]
    a1 = title["art"][1]
    poster = title["poster"]
    modal_bg = f"url({poster})" if poster else f"linear-gradient(135deg, {a0}, {a1})"

    def _setup():
        def on_key(e):
            if e.key == "Escape":
                on_close()

        document.addEventListener("keydown", on_key)
        prev = document.body.style.overflow
        document.body.style.overflow = "hidden"

        def _cleanup():
            document.removeEventListener("keydown", on_key)
            document.body.style.overflow = prev

        return _cleanup

    ue(_setup, [on_close])

    return create_portal(
        div(
            cn="nf-backdrop",
            data_testid="nf-modal-backdrop",
            oc=on_close,
            div(
                cn="nf-modal",
                data_testid="nf-modal",
                role="dialog",
                aria_modal="true",
                oc=lambda e: e.stopPropagation(),
                button(
                    cn="nf-modal-close",
                    data_testid="nf-modal-close",
                    aria_label="Close",
                    oc=on_close,
                    "✕",
                ),
                div(
                    cn="nf-modal-art",
                    st={
                        "backgroundImage": modal_bg,
                        "backgroundColor": a0,
                        "backgroundSize": "cover",
                        "backgroundPosition": "center",
                    },
                    span(cn="nf-card-name", title["name"]),
                ),
                div(
                    cn="nf-modal-body",
                    video(
                        cn="nf-modal-preview",
                        data_testid="nf-modal-preview",
                        src="/media/sample.webm",
                        muted=True,
                        auto_play=True,
                        loop=True,
                        plays_inline=True,
                    ),
                    h2(data_testid="nf-modal-title", title["name"]),
                    p(
                        cn="nf-meta",
                        span(cn="nf-match", f"{title['match']}% Match"),
                        span(title["year"]),
                        span(cn="nf-maturity", title["maturity"]),
                        span(title["duration"]),
                    ),
                    p(cn="nf-modal-desc", data_testid="nf-modal-desc", title["description"]),
                    p(cn="nf-modal-genres", " | ".join(title["genres"])),
                    button(
                        cn="nf-btn nf-btn-info",
                        data_testid="nf-modal-toggle",
                        oc=lambda: ctx["toggle"](tid),
                        "✓ In My List" if in_list else "+ Add to My List",
                    ),
                ),
            ),
        ),
        document.body,
    )


@c
def NetflixBrowse(titles, rows, hero_id):
    ctx = use_my_list()
    sel, set_sel = us(None)

    def close_modal():
        set_sel(None)

    my_titles = [titles[tid] for tid in ctx["ids"]]
    return div(
        cn="nf-app",
        data_testid="nf-app",
        Hero(title=titles[hero_id], on_more_info=set_sel),
        main(
            cn="nf-rows",
            section(
                cn="nf-mylist",
                data_testid="nf-mylist-section",
                Carousel(row_id="my-list", label="My List", titles=my_titles, on_open=set_sel)
                if len(my_titles) > 0
                else div(
                    cn="nf-row",
                    h2(cn="nf-row-label", "My List"),
                    p(
                        cn="nf-mylist-empty",
                        data_testid="nf-mylist-empty",
                        "Your list is empty. Add titles with the + button.",
                    ),
                ),
            ),
            *[
                Carousel(
                    key=r["id"],
                    row_id=r["id"],
                    label=r["label"],
                    titles=[titles[tid] for tid in r["title_ids"]],
                    on_open=set_sel,
                )
                for r in rows
            ],
        ),
        sel and DetailModal(title=sel, on_close=close_modal),
    )


@c
def NetflixApp(titles, rows, hero_id):
    return MyListProvider(
        NetflixBrowse(titles=titles, rows=rows, hero_id=hero_id),
    )


__default__ = NetflixApp
