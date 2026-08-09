<!-- exp=baseline-001 task=macro_movie_rows cond=psc sample=2 model=claude-opus-4-8 cost_usd=0.0453615 -->
```python
"use client"
R*

MOVIES = {
    "trending": [
        {"title": "Neon Skyline", "year": 2023, "rating": 8.4,
         "description": "A rogue architect races to rebuild a city before dawn."},
        {"title": "Quiet Harbor", "year": 2021, "rating": 7.9,
         "description": "Two strangers share a lighthouse through a long winter."},
        {"title": "Iron Meridian", "year": 2022, "rating": 8.1,
         "description": "A miner uncovers a signal buried beneath the polar ice."},
        {"title": "Paper Kingdoms", "year": 2020, "rating": 7.2,
         "description": "Rival mapmakers redraw a nation one border at a time."},
    ],
    "new": [
        {"title": "Glass Orchard", "year": 2024, "rating": 8.7,
         "description": "A botanist grows an impossible garden inside a comet."},
        {"title": "Last Transmission", "year": 2024, "rating": 8.0,
         "description": "A night-shift operator answers a call from the future."},
        {"title": "Salt & Ember", "year": 2024, "rating": 7.6,
         "description": "A wandering cook trades recipes for safe passage home."},
        {"title": "The Ninth Gate Key", "year": 2024, "rating": 8.3,
         "description": "A locksmith inherits a door that opens onto memories."},
    ],
}

FEATURED = {
    "title": "Neon Skyline",
    "year": 2023,
    "rating": 8.4,
    "description": "In a city that never sleeps, a rogue architect races to rebuild the skyline before it collapses at dawn.",
}


def movie_card(m, on_open):
    return div(cn="movie-card", key=m["title"], oc=lambda: on_open(m),
        div(cn="movie-card-poster",
            span(cn="movie-card-rating", f"{m['rating']:.1f}"),
        ),
        div(cn="movie-card-meta",
            span(cn="movie-card-title", m["title"]),
            span(cn="movie-card-year", str(m["year"])),
        ),
    )


def movie_row(label, movies, on_open):
    return section(cn="movie-row", key=label,
        h3(cn="movie-row-title", label),
        div(cn="movie-row-track",
            *[movie_card(m, on_open) for m in movies]),
    )


@c
def MovieBrowser():
    selected, set_selected = us(None)

    def open_detail(m):
        set_selected(m)

    def close_detail():
        set_selected(None)

    return div(cn="movie-browser",
        section(cn="hero",
            div(cn="hero-content",
                span(cn="hero-badge", "Featured"),
                h1(cn="hero-title", FEATURED["title"]),
                div(cn="hero-meta",
                    span(cn="hero-year", str(FEATURED["year"])),
                    span(cn="hero-rating", f"★ {FEATURED['rating']:.1f}"),
                ),
                p(cn="hero-description", FEATURED["description"]),
            ),
        ),
        movie_row("Trending", MOVIES["trending"], open_detail),
        movie_row("New", MOVIES["new"], open_detail),
        (div(cn="detail-overlay",
            div(cn="detail-panel",
                button(cn="detail-close", oc=lambda: close_detail(), "Close"),
                h2(cn="detail-title", selected["title"]),
                div(cn="detail-meta",
                    span(cn="detail-year", str(selected["year"])),
                    span(cn="detail-rating", f"★ {selected['rating']:.1f}"),
                ),
                p(cn="detail-description", selected["description"]),
            ),
        ) if selected else None),
    )
```
