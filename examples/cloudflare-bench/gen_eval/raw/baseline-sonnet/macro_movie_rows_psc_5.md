<!-- exp=baseline-sonnet task=macro_movie_rows cond=psc sample=5 model=claude-sonnet-5 cost_usd=0.031089600000000002 -->
```python
"use client"
R*

MOVIES = {
    "trending": [
        {"id": 1, "title": "Neon Horizon", "year": 2024, "rating": 8.7,
         "description": "A hacker uncovers a conspiracy in a city run by rogue AI."},
        {"id": 2, "title": "Glass Tide", "year": 2023, "rating": 7.9,
         "description": "Two rival surfers race to save their coastal town from a corporate buyout."},
        {"id": 3, "title": "Ember Road", "year": 2022, "rating": 8.2,
         "description": "A convoy of survivors crosses a burning continent in search of refuge."},
        {"id": 4, "title": "Static Bloom", "year": 2024, "rating": 7.5,
         "description": "A botanist discovers a flower that rewrites memories."},
    ],
    "new": [
        {"id": 5, "title": "Midnight Ferry", "year": 2026, "rating": 8.0,
         "description": "A late-night ferry ride turns into a hostage negotiation."},
        {"id": 6, "title": "Paper Wolves", "year": 2026, "rating": 7.6,
         "description": "Origami artists moonlight as an underground heist crew."},
        {"id": 7, "title": "Vacant Signal", "year": 2025, "rating": 8.4,
         "description": "A radio operator picks up broadcasts from a town that no longer exists."},
        {"id": 8, "title": "Low Orbit", "year": 2025, "rating": 7.8,
         "description": "A skeleton crew keeps a dying space station running one more year."},
    ],
}

FEATURED = {
    "title": "Neon Horizon",
    "description": "In a city ruled by rogue AI, one hacker's discovery could tear it all down. "
                    "Winner of three festival awards, Neon Horizon redefines the cyberpunk thriller.",
}


def MovieCard(movie, on_select):
    return div(cn="movie-card", oc=lambda: on_select(movie), key=movie["id"],
        div(cn="movie-card-poster", movie["title"][0]),
        div(cn="movie-card-title", movie["title"]),
        div(cn="movie-card-meta", f"{movie['year']} · {movie['rating']}★"),
    )


def MovieRow(title, movies, on_select):
    return div(cn="movie-row",
        h2(cn="movie-row-title", title),
        div(cn="movie-row-track",
            *[MovieCard(m, on_select) for m in movies],
        ),
    )


def DetailPanel(movie, on_close):
    return div(cn="detail-panel",
        button(cn="detail-close", oc=on_close, "Close"),
        h2(cn="detail-title", movie["title"]),
        div(cn="detail-meta", f"{movie['year']} · {movie['rating']}★"),
        p(cn="detail-description", movie["description"]),
    )


@c
def MovieBrowser():
    selected, set_selected = us(None)

    def select_movie(movie):
        set_selected(movie)

    def close_panel():
        set_selected(None)

    return div(cn="movie-browser",
        div(cn="hero",
            h1(cn="hero-title", FEATURED["title"]),
            p(cn="hero-description", FEATURED["description"]),
        ),
        DetailPanel(selected, close_panel) if selected else None,
        MovieRow("Trending", MOVIES["trending"], select_movie),
        MovieRow("New", MOVIES["new"], select_movie),
    )
```
