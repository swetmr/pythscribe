<!-- exp=baseline-sonnet task=macro_movie_rows cond=ps sample=3 model=claude-sonnet-5 cost_usd=0.0331206 -->
```python
"use client"

from pyths.react import component, use_state

MOVIES = {
    "trending": [
        {"id": 1, "title": "Neon Horizon", "year": 2024, "rating": 8.7,
         "description": "A hacker uncovers a conspiracy hidden inside a city-wide neural network."},
        {"id": 2, "title": "Glass Tide", "year": 2023, "rating": 7.9,
         "description": "Two rival captains race across a flooded world to reach the last dry land."},
        {"id": 3, "title": "Ember Court", "year": 2022, "rating": 8.2,
         "description": "A deposed queen builds an underground resistance from the ashes of her palace."},
        {"id": 4, "title": "Static Bloom", "year": 2024, "rating": 7.5,
         "description": "A botanist discovers a flower that rewrites memories when it blooms."},
    ],
    "new": [
        {"id": 5, "title": "Midnight Circuit", "year": 2026, "rating": 8.0,
         "description": "An underground street-racing crew stumbles into an international heist."},
        {"id": 6, "title": "Paper Moons", "year": 2026, "rating": 7.6,
         "description": "A folded-note love story unfolds across three decades and two continents."},
        {"id": 7, "title": "Driftwood", "year": 2025, "rating": 8.4,
         "description": "A shipwreck survivor rebuilds a life on an island that isn't as empty as it seems."},
        {"id": 8, "title": "Iron Season", "year": 2025, "rating": 7.8,
         "description": "A retired blacksmith is pulled back into a war she thought she'd escaped."},
    ],
}

FEATURED = {
    "id": 0, "title": "Ember Court", "year": 2022, "rating": 8.2,
    "description": "A deposed queen builds an underground resistance from the ashes of her palace. "
                    "As old allies turn to enemies, she must decide how much of herself she's willing to burn to reclaim her throne.",
}


def star_label(rating):
    return f"★ {rating:.1f}"


def MovieCard(movie, on_select):
    return div(
        key=movie["id"],
        class_name="movie-card",
        on_click=lambda: on_select(movie),
        div(class_name="movie-card-thumb", movie["title"][0]),
        div(class_name="movie-card-info",
            div(class_name="movie-card-title", movie["title"]),
            div(class_name="movie-card-meta", f"{movie['year']} · {star_label(movie['rating'])}"),
        ),
    )


def MovieRow(title, movies, on_select):
    return div(class_name="movie-row",
        h2(class_name="movie-row-title", title),
        div(class_name="movie-row-track",
            *[MovieCard(m, on_select) for m in movies],
        ),
    )


def DetailPanel(movie, on_close):
    return div(class_name="detail-panel",
        div(class_name="detail-panel-header",
            h2(movie["title"]),
            button(class_name="detail-close-btn", on_click=lambda: on_close(), "Close"),
        ),
        div(class_name="detail-panel-meta", f"{movie['year']} · {star_label(movie['rating'])}"),
        p(class_name="detail-panel-description", movie["description"]),
    )


@component
def MovieBrowser():
    selected, set_selected = use_state(None)

    def select_movie(movie):
        set_selected(movie)

    def close_panel():
        set_selected(None)

    return div(class_name="movie-browser",
        div(class_name="hero",
            div(class_name="hero-content",
                h1(class_name="hero-title", FEATURED["title"]),
                div(class_name="hero-meta", f"{FEATURED['year']} · {star_label(FEATURED['rating'])}"),
                p(class_name="hero-description", FEATURED["description"]),
            ),
        ),
        DetailPanel(selected, close_panel) if selected else None,
        MovieRow("Trending", MOVIES["trending"], select_movie),
        MovieRow("New", MOVIES["new"], select_movie),
    )
```
