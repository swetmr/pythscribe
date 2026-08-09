<!-- exp=baseline-001 task=macro_movie_rows cond=psc sample=5 model=claude-opus-4-8 cost_usd=0.0459365 -->
```python
"use client"
R*

MOVIES = {
    "trending": [
        {"title": "Neon Horizon", "year": 2024, "rating": 8.4,
         "desc": "A rogue pilot races across a fractured sky-city to outrun her own past."},
        {"title": "The Silent Vault", "year": 2023, "rating": 7.9,
         "desc": "A master thief plans one last heist inside a bank that never forgets."},
        {"title": "Crimson Tide Rising", "year": 2025, "rating": 8.1,
         "desc": "Two rival captains are forced to share a lifeboat as the storm closes in."},
        {"title": "Paper Moons", "year": 2022, "rating": 7.2,
         "desc": "A traveling puppeteer stumbles into a town where every wish comes true."},
    ],
    "new": [
        {"title": "Glass Country", "year": 2026, "rating": 8.8,
         "desc": "In a nation made of mirrors, a girl discovers the one face that never reflects."},
        {"title": "Undertow", "year": 2026, "rating": 7.6,
         "desc": "A marine biologist chases a signal from the deepest trench on Earth."},
        {"title": "Ashfall", "year": 2026, "rating": 8.0,
         "desc": "Survivors of a dormant volcano's return rebuild trust one winter at a time."},
        {"title": "The Ninth Letter", "year": 2026, "rating": 7.4,
         "desc": "A retired codebreaker receives a message meant for someone long dead."},
    ],
}

FEATURED = {
    "title": "Neon Horizon",
    "year": 2024,
    "rating": 8.4,
    "desc": "A rogue pilot races across a fractured sky-city to outrun her own past — a neon-drenched chase through a world that never sleeps.",
}


def movie_card(movie, on_open):
    return div(cn="movie-card", oc=lambda: on_open(movie),
        div(cn="movie-poster",
            span(cn="movie-rating", f"★ {movie['rating']:.1f}"),
        ),
        div(cn="movie-meta",
            span(cn="movie-title", movie["title"]),
            span(cn="movie-year", str(movie["year"])),
        ),
    )


def movie_row(label, movies, on_open):
    return section(cn="movie-row",
        h3(cn="row-title", label),
        div(cn="row-track",
            *[movie_card(m, on_open) for m in movies],
        ),
    )


@c
def MovieBrowser():
    selected, set_selected = us(None)

    def open_movie(movie):
        set_selected(movie)

    def close_panel():
        set_selected(None)

    return div(cn="movie-browser",
        section(cn="hero",
            div(cn="hero-overlay",
                h1(cn="hero-title", FEATURED["title"]),
                div(cn="hero-sub",
                    span(cn="hero-year", str(FEATURED["year"])),
                    span(cn="hero-rating", f"★ {FEATURED['rating']:.1f}"),
                ),
                p(cn="hero-desc", FEATURED["desc"]),
                button(cn="hero-play", "▶ Play"),
            ),
        ),
        movie_row("Trending", MOVIES["trending"], open_movie),
        movie_row("New", MOVIES["new"], open_movie),
        div(cn="detail-panel",
            div(cn="detail-card",
                button(cn="detail-close", oc=lambda: close_panel(), "✕ Close"),
                h2(cn="detail-title", selected["title"]),
                div(cn="detail-meta",
                    span(cn="detail-year", str(selected["year"])),
                    span(cn="detail-rating", f"★ {selected['rating']:.1f}"),
                ),
                p(cn="detail-desc", selected["desc"]),
            ),
        ) if selected else None,
    )
```
