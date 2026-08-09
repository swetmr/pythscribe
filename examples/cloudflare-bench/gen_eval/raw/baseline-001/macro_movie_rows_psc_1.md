<!-- exp=baseline-001 task=macro_movie_rows cond=psc sample=1 model=claude-opus-4-8 cost_usd=0.0902955 -->
```python
"use client"
R*

MOVIES = {
    "trending": [
        {"title": "Neon Horizon", "year": 2023, "rating": 8.4, "desc": "A hacker uncovers a conspiracy beneath a glowing megacity."},
        {"title": "Silent Orbit", "year": 2021, "rating": 7.9, "desc": "Stranded astronauts race to repair a failing station."},
        {"title": "Crimson Vale", "year": 2022, "rating": 8.1, "desc": "A frontier town guards a secret in the red desert."},
        {"title": "Paper Kingdoms", "year": 2020, "rating": 7.2, "desc": "Rival dynasties battle for a fragile empire."},
    ],
    "new": [
        {"title": "Glass Meridian", "year": 2024, "rating": 8.7, "desc": "Two strangers meet across a shifting timeline."},
        {"title": "Undertow", "year": 2024, "rating": 7.6, "desc": "A diver confronts what the ocean refuses to return."},
        {"title": "Ashfall County", "year": 2023, "rating": 8.0, "desc": "A sheriff faces a reckoning after the volcano wakes."},
        {"title": "Bright Static", "year": 2024, "rating": 7.4, "desc": "A radio host chases a signal that shouldn't exist."},
    ],
}

FEATURED = {
    "title": "Neon Horizon",
    "year": 2023,
    "rating": 8.4,
    "desc": "In a city that never sleeps, one hacker holds the key to bringing it all down. A sleek, breathless thriller lit by ten million screens.",
}

def MovieCard(movie, on_open):
    return div(cn="movie-card", oc=lambda: on_open(movie),
        div(cn="movie-card-poster",
            span(cn="movie-card-title", movie["title"]),
        ),
        div(cn="movie-card-meta",
            span(cn="movie-card-year", str(movie["year"])),
            span(cn="movie-card-rating", f"★ {movie['rating']:.1f}"),
        ),
    )

def MovieRow(label, movies, on_open):
    return section(cn="movie-row",
        h2(cn="movie-row-title", label),
        div(cn="movie-row-track",
            *[MovieCard(m, on_open) for i, m in enumerate(movies)]),
    )

@c
def MovieBrowser():
    selected, set_selected = us(None)

    def open_movie(movie):
        set_selected(movie)

    def close():
        set_selected(None)

    return div(cn="movie-browser",
        section(cn="hero",
            div(cn="hero-content",
                h1(cn="hero-title", FEATURED["title"]),
                div(cn="hero-meta",
                    span(cn="hero-year", str(FEATURED["year"])),
                    span(cn="hero-rating", f"★ {FEATURED['rating']:.1f}"),
                ),
                p(cn="hero-desc", FEATURED["desc"]),
            ),
        ),
        MovieRow("Trending", MOVIES["trending"], open_movie),
        MovieRow("New", MOVIES["new"], open_movie),
        div(cn="detail-panel",
            div(cn="detail-card",
                h2(cn="detail-title", selected["title"]),
                div(cn="detail-meta",
                    span(cn="detail-year", str(selected["year"])),
                    span(cn="detail-rating", f"★ {selected['rating']:.1f}"),
                ),
                p(cn="detail-desc", selected["desc"]),
                button(cn="detail-close", oc=lambda: close(), "Close"),
            ),
        ) if selected else None,
    )
```
