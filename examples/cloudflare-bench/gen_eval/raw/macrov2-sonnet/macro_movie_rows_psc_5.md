<!-- exp=macrov2-sonnet task=macro_movie_rows cond=psc sample=5 model=claude-sonnet-5 cost_usd=0.032256900000000005 -->
```python
"use client"
R*

MOVIES = {
    "trending": [
        {"id": 1, "title": "Nebula Drift", "year": 2024, "rating": 8.4, "description": "A crew chasing a rogue signal finds it's chasing them back."},
        {"id": 2, "title": "Glass Horizon", "year": 2023, "rating": 7.9, "description": "Two rival architects rebuild a city divided by an unnatural border."},
        {"id": 3, "title": "Ashen Roads", "year": 2022, "rating": 8.1, "description": "A courier crosses a scorched continent to deliver one final letter."},
        {"id": 4, "title": "Paper Tigers", "year": 2025, "rating": 7.5, "description": "Three con artists plan the heist of a museum that doesn't exist."},
    ],
    "new": [
        {"id": 5, "title": "Low Tide", "year": 2026, "rating": 7.2, "description": "A retired diver is pulled back into the sea by a debt she can't repay."},
        {"id": 6, "title": "Static Bloom", "year": 2026, "rating": 8.0, "description": "A radio operator in a dead zone starts hearing broadcasts from the future."},
        {"id": 7, "title": "Iron Orchard", "year": 2025, "rating": 6.9, "description": "A farming family fights to keep their land in a world run by machines."},
        {"id": 8, "title": "Quiet Fracture", "year": 2026, "rating": 8.3, "description": "A therapist begins to suspect her newest patient is rewriting her memories."},
    ],
}

FEATURED = {"id": 0, "title": "Nebula Drift", "year": 2024, "rating": 8.4, "description": "A crew chasing a rogue signal finds it's chasing them back. When the signal leads them past the edge of the charted stars, they must decide whether to turn home or follow it into the unknown."}

@c
def MovieCard(movie, on_select):
    return div(cn="movie-card", oc=lambda: on_select(movie),
        div(cn="movie-card-poster", movie["title"][0]),
        div(cn="movie-card-info",
            div(cn="movie-card-title", movie["title"]),
            div(cn="movie-card-meta", f"{movie['year']} · {movie['rating']}★"),
        ),
    )

@c
def MovieRow(title, movies, on_select):
    return section(cn="movie-row",
        h2(cn="movie-row-title", title),
        div(cn="movie-row-track",
            *[MovieCard(movie=m, on_select=on_select, key=m["id"]) for m in movies],
        ),
    )

@c
def DetailPanel(movie, on_close):
    return div(cn="detail-panel",
        button(cn="detail-panel-close", oc=lambda: on_close(), "Close"),
        h2(cn="detail-panel-title", movie["title"]),
        div(cn="detail-panel-meta", f"{movie['year']} · {movie['rating']}★"),
        p(cn="detail-panel-desc", movie["description"]),
    )

@c
def MovieBrowser():
    selected, set_selected = us(None)

    return div(cn="movie-browser",
        div(cn="hero",
            h1(cn="hero-title", FEATURED["title"]),
            div(cn="hero-meta", f"{FEATURED['year']} · {FEATURED['rating']}★"),
            p(cn="hero-desc", FEATURED["description"]),
        ),
        DetailPanel(movie=selected, on_close=lambda: set_selected(None)) if selected else None,
        MovieRow(title="Trending", movies=MOVIES["trending"], on_select=lambda m: set_selected(m)),
        MovieRow(title="New", movies=MOVIES["new"], on_select=lambda m: set_selected(m)),
    )
```
