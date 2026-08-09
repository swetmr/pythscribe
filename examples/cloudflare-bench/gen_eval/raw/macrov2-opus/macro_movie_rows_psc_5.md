<!-- exp=macrov2-opus task=macro_movie_rows cond=psc sample=5 model=claude-opus-4-8 cost_usd=0.04996949999999999 -->
```python
"use client"
R*

TRENDING = [
    {"title": "Neon Skyline", "year": 2023, "rating": 8.1, "desc": "A courier races through a rain-soaked megacity guarding a secret that could topple the corporate elite."},
    {"title": "Iron Tide", "year": 2021, "rating": 7.4, "desc": "Two rival captains must join forces when a storm strands them on a hostile coast."},
    {"title": "Paper Lanterns", "year": 2022, "rating": 8.6, "desc": "A quiet festival town confronts its past when an old friend returns home."},
    {"title": "Glasshouse", "year": 2020, "rating": 6.9, "desc": "A family sealed inside a self-sufficient dome begins to unravel as trust erodes."},
    {"title": "Static Bloom", "year": 2024, "rating": 7.8, "desc": "A radio engineer discovers a signal that seems to predict the future."},
]

NEW = [
    {"title": "Cold Harbor", "year": 2025, "rating": 7.2, "desc": "A detective works one last winter case in a town that keeps its secrets frozen."},
    {"title": "The Long Green", "year": 2025, "rating": 8.3, "desc": "An heir returns to run a failing golf resort and finds a hidden ledger."},
    {"title": "Marrow", "year": 2024, "rating": 8.0, "desc": "A field medic in a remote outpost fights to keep a fragile peace alive."},
    {"title": "Halcyon Drift", "year": 2025, "rating": 7.6, "desc": "A salvage crew wakes a dormant ship carrying more than cargo."},
    {"title": "Ember Court", "year": 2024, "rating": 8.4, "desc": "Rival houses vie for a throne as an ancient fire stirs beneath the capital."},
]

FEATURED = {
    "title": "Paper Lanterns",
    "year": 2022,
    "rating": 8.6,
    "desc": "A quiet festival town confronts its past when an old friend returns home, and a single night of light changes everything.",
}


@c
def MovieCard(movie, on_open):
    return button(cn="movie-card", oc=lambda: on_open(movie),
        div(cn="movie-card-poster"),
        div(cn="movie-card-body",
            span(cn="movie-card-title", movie["title"]),
            span(cn="movie-card-meta", f"{movie['year']} · ★ {movie['rating']:.1f}"),
        ),
    )


@c
def MovieRow(title, movies, on_open):
    return section(cn="movie-row",
        h3(cn="movie-row-title", title),
        div(cn="movie-row-track",
            *[MovieCard(key=m["title"], movie=m, on_open=on_open) for m in movies],
        ),
    )


@c
def DetailPanel(movie, on_close):
    return div(cn="detail-panel",
        div(cn="detail-panel-inner",
            div(cn="detail-panel-head",
                h2(cn="detail-panel-title", movie["title"]),
                button(cn="detail-panel-close", oc=lambda: on_close(), "Close"),
            ),
            div(cn="detail-panel-meta",
                span(cn="detail-panel-year", str(movie["year"])),
                span(cn="detail-panel-rating", f"★ {movie['rating']:.1f}"),
            ),
            p(cn="detail-panel-desc", movie["desc"]),
        ),
    )


@c
def MovieBrowser():
    selected, set_selected = us(None)

    return div(cn="movie-browser",
        section(cn="hero",
            div(cn="hero-overlay",
                span(cn="hero-badge", "Featured"),
                h1(cn="hero-title", FEATURED["title"]),
                p(cn="hero-meta", f"{FEATURED['year']} · ★ {FEATURED['rating']:.1f}"),
                p(cn="hero-desc", FEATURED["desc"]),
                button(cn="hero-play", oc=lambda: set_selected(FEATURED), "More Info"),
            ),
        ),
        MovieRow(title="Trending", movies=TRENDING, on_open=lambda m: set_selected(m)),
        MovieRow(title="New", movies=NEW, on_open=lambda m: set_selected(m)),
        DetailPanel(movie=selected, on_close=lambda: set_selected(None)) if selected else None,
    )
```
