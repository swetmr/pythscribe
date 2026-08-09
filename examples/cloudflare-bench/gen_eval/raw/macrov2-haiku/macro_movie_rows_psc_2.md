<!-- exp=macrov2-haiku task=macro_movie_rows cond=psc sample=2 model=claude-haiku-4-5-20251001 cost_usd=0.046980900000000006 -->
```python
"use client"
R*

FEATURED = {
    "id": "hero-1",
    "title": "The Quantum Paradox",
    "year": 2024,
    "rating": "PG-13",
    "description": "A mind-bending thriller about scientists uncovering a dangerous anomaly in space-time.",
}

TRENDING = [
    {"id": "t1", "title": "Neon City", "year": 2024, "rating": "R", "description": "A cyberpunk noir set in a sprawling megacity."},
    {"id": "t2", "title": "Echoes", "year": 2023, "rating": "PG-13", "description": "A haunting mystery where nothing is as it seems."},
    {"id": "t3", "title": "The Last Stand", "year": 2024, "rating": "R", "description": "An epic final battle between ancient forces."},
    {"id": "t4", "title": "Dreams Unraveled", "year": 2023, "rating": "PG", "description": "A surreal journey through the subconscious mind."},
    {"id": "t5", "title": "Steel Horizon", "year": 2024, "rating": "PG-13", "description": "Post-apocalyptic survival on the edge of civilization."},
]

NEW = [
    {"id": "n1", "title": "Crimson Web", "year": 2025, "rating": "R", "description": "An undercover spy thriller with unexpected twists."},
    {"id": "n2", "title": "Luminescence", "year": 2025, "rating": "PG", "description": "A family adventure in a magical underwater kingdom."},
    {"id": "n3", "title": "Fractured Souls", "year": 2025, "rating": "R", "description": "A dark psychological drama about identity and memory."},
    {"id": "n4", "title": "Starbound", "year": 2025, "rating": "PG-13", "description": "A grand space opera exploring new frontiers."},
    {"id": "n5", "title": "The Forgotten", "year": 2025, "rating": "PG-13", "description": "A touching story of lost love and redemption."},
]

@c
def MovieCard(movie, on_click):
    return div(cn="movie-card", oc=on_click,
        div(cn="movie-card-poster",
            div(cn="movie-card-title", movie["title"]),
        ),
        div(cn="movie-card-info",
            p(f"{movie['year']} • {movie['rating']}"),
        ),
    )

@c
def MovieRow(title, movies, on_select):
    return div(cn="movie-row",
        h3(title),
        div(cn="movie-carousel",
            *[MovieCard(movie, lambda: on_select(movie)) for movie in movies],
        ),
    )

@c
def DetailPanel(movie, on_close):
    return div(cn="detail-overlay", oc=on_close,
        div(cn="detail-panel", oc=lambda e: e.stopPropagation(),
            button(cn="detail-close", oc=on_close, "×"),
            h2(movie["title"]),
            p(cn="detail-meta", f"{movie['year']} • {movie['rating']}"),
            p(cn="detail-description", movie["description"]),
        ),
    )

@c
def MovieBrowser():
    selected, set_selected = us(None)

    return div(cn="movie-browser",
        div(cn="hero-section",
            div(cn="hero-content",
                h1(FEATURED["title"]),
                p(cn="hero-description", FEATURED["description"]),
                button(cn="hero-btn", oc=lambda: set_selected(FEATURED), "Watch Now"),
            ),
        ),
        div(cn="browse-section",
            MovieRow("Trending", TRENDING, set_selected),
            MovieRow("New", NEW, set_selected),
        ),
        selected and DetailPanel(selected, lambda: set_selected(None)) or None,
    )
```
