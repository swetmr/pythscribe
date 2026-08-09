<!-- exp=macrov2-sonnet task=macro_movie_rows cond=psc sample=1 model=claude-sonnet-5 cost_usd=0.0590436 -->
```python
"use client"
R*

MOVIES = {
    "trending": [
        {"id": 1, "title": "Nebula Drift", "year": 2024, "rating": 8.7, "description": "A stranded crew races to repair their ship before a dying star consumes the system."},
        {"id": 2, "title": "Iron Tide", "year": 2022, "rating": 7.9, "description": "A dockworker uncovers a smuggling ring that threatens her entire city."},
        {"id": 3, "title": "Glass Horizon", "year": 2023, "rating": 8.2, "description": "Twin sisters separated at birth reunite to solve their mother's disappearance."},
        {"id": 4, "title": "Crimson Static", "year": 2021, "rating": 7.3, "description": "A radio host broadcasts warnings from a future that hasn't happened yet."},
    ],
    "new": [
        {"id": 5, "title": "Paper Lanterns", "year": 2026, "rating": 8.0, "description": "Three strangers share a train car and a secret that binds their fates."},
        {"id": 6, "title": "Winter Static", "year": 2026, "rating": 7.6, "description": "A retired detective is pulled back for one impossible case."},
        {"id": 7, "title": "The Long Reef", "year": 2025, "rating": 8.4, "description": "Marine researchers discover a structure that shouldn't exist beneath the sea."},
        {"id": 8, "title": "Vellum & Ash", "year": 2025, "rating": 7.8, "description": "A forger's final commission forces her to confront her past."},
    ],
}

FEATURED = {"title": "Nebula Drift", "year": 2024, "rating": 8.7, "description": "A stranded crew races to repair their ship before a dying star consumes the system."}

@c
def MovieCard(movie, on_select):
    return div(cn="movie-card", oc=lambda: on_select(movie),
        div(cn="movie-card-thumb", f"{movie['title'][0]}"),
        div(cn="movie-card-info",
            h4(movie["title"]),
            p(f"{movie['year']} · {movie['rating']:.1f}"),
        ),
    )

@c
def MovieRow(title, movies, on_select):
    return section(cn="movie-row",
        h3(title),
        div(cn="movie-row-track",
            *[MovieCard(movie=m, on_select=on_select, key=m["id"]) for m in movies]
        ),
    )

@c
def DetailPanel(movie, on_close):
    return div(cn="detail-panel",
        button(cn="detail-close", oc=lambda: on_close(), "Close"),
        h2(movie["title"]),
        p(cn="detail-meta", f"{movie['year']} · {movie['rating']:.1f}"),
        p(cn="detail-description", movie["description"]),
    )

@c
def MovieBrowser():
    selected, set_selected = us(None)

    def select_movie(m):
        set_selected(m)

    def close_panel():
        set_selected(None)

    return div(cn="movie-browser",
        div(cn="hero",
            h1(FEATURED["title"]),
            p(cn="hero-meta", f"{FEATURED['year']} · {FEATURED['rating']:.1f}"),
            p(cn="hero-description", FEATURED["description"]),
        ),
        DetailPanel(movie=selected, on_close=close_panel) if selected else None,
        MovieRow(title="Trending", movies=MOVIES["trending"], on_select=select_movie),
        MovieRow(title="New", movies=MOVIES["new"], on_select=select_movie),
    )
```
