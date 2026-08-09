<!-- exp=macrov2-opus task=macro_movie_rows cond=psc sample=1 model=claude-opus-4-8 cost_usd=0.09496399999999999 -->
```python
"use client"
R*

TRENDING = [
    {"title": "Neon Skyline", "year": 2023, "rating": 8.4, "desc": "A courier races through a rain-soaked megacity to outrun a syndicate."},
    {"title": "Quiet Harbor", "year": 2021, "rating": 7.9, "desc": "Two strangers rebuild a fishing village over one long summer."},
    {"title": "Ironwood", "year": 2022, "rating": 8.1, "desc": "A ranger defends an ancient forest from a relentless logging empire."},
    {"title": "Last Signal", "year": 2020, "rating": 7.5, "desc": "A lone operator picks up a transmission that should not exist."},
    {"title": "Paper Moons", "year": 2024, "rating": 8.7, "desc": "A traveling puppeteer stitches together the towns she passes through."},
]

NEW = [
    {"title": "Glass Coast", "year": 2025, "rating": 8.0, "desc": "A marine biologist chases a vanishing reef and her own past."},
    {"title": "Understory", "year": 2025, "rating": 7.7, "desc": "A mycologist uncovers a network no one was meant to find."},
    {"title": "Cinder Lane", "year": 2024, "rating": 8.3, "desc": "A firefighter returns to the block that shaped her."},
    {"title": "Northbound", "year": 2025, "rating": 7.4, "desc": "A trucker and a runaway share one frozen highway."},
    {"title": "Halcyon", "year": 2025, "rating": 8.6, "desc": "A composer hears the city as a symphony coming apart."},
]

FEATURED = {
    "title": "Neon Skyline",
    "year": 2023,
    "rating": 8.4,
    "desc": "In a rain-soaked megacity of endless neon, a courier stumbles onto a package that a ruthless syndicate will burn the whole grid to recover.",
}


@c
def Card(movie, on_open):
    return div(cn="movie-card", oc=lambda: on_open(movie),
        div(cn="movie-card-poster"),
        div(cn="movie-card-body",
            span(cn="movie-card-title", movie["title"]),
            span(cn="movie-card-meta", f"{movie['year']} · ★ {movie['rating']:.1f}"),
        ),
    )


@c
def Row(title, movies, on_open):
    return section(cn="movie-row",
        h3(cn="movie-row-title", title),
        div(cn="movie-row-track",
            *[Card(movie=m, on_open=on_open) for m in movies],
        ),
    )


@c
def DetailPanel(movie, on_close):
    return div(cn="detail-panel",
        div(cn="detail-panel-inner",
            button(cn="detail-close", oc=lambda: on_close(), "Close"),
            h2(cn="detail-title", movie["title"]),
            div(cn="detail-meta", f"{movie['year']} · ★ {movie['rating']:.1f}"),
            p(cn="detail-desc", movie["desc"]),
        ),
    )


@c
def MovieBrowser():
    selected, set_selected = us(None)

    return div(cn="movie-browser",
        section(cn="hero",
            div(cn="hero-backdrop"),
            div(cn="hero-content",
                span(cn="hero-badge", "Featured"),
                h1(cn="hero-title", FEATURED["title"]),
                div(cn="hero-meta", f"{FEATURED['year']} · ★ {FEATURED['rating']:.1f}"),
                p(cn="hero-desc", FEATURED["desc"]),
            ),
        ),
        DetailPanel(movie=selected, on_close=lambda: set_selected(None)) if selected else None,
        div(cn="movie-rows",
            Row(title="Trending", movies=TRENDING, on_open=lambda m: set_selected(m)),
            Row(title="New", movies=NEW, on_open=lambda m: set_selected(m)),
        ),
    )
```
