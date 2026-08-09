<!-- exp=macrov2-sonnet task=macro_movie_rows cond=ps sample=5 model=claude-sonnet-5 cost_usd=0.0346503 -->
```python
"use client"

from pyths.react import component, use_state

MOVIES = {
    "trending": [
        {"id": 1, "title": "Neon Horizon", "year": 2023, "rating": 8.4,
         "description": "A hacker uncovers a conspiracy hidden inside a city-wide neural network."},
        {"id": 2, "title": "Glass Tide", "year": 2021, "rating": 7.6,
         "description": "Two rival smugglers race across a flooded coastline to recover a lost cargo."},
        {"id": 3, "title": "Ironbound", "year": 2019, "rating": 8.1,
         "description": "A disgraced blacksmith forges a weapon to reclaim his exiled kingdom."},
        {"id": 4, "title": "Paper Moons", "year": 2022, "rating": 7.2,
         "description": "An origami artist discovers her folds can bend the fabric of memory."},
        {"id": 5, "title": "Static Bloom", "year": 2020, "rating": 6.9,
         "description": "A radio operator picks up transmissions from a town that vanished decades ago."},
    ],
    "new": [
        {"id": 6, "title": "Salt & Circuit", "year": 2024, "rating": 7.8,
         "description": "A retired engineer builds a companion robot to sail the last free ocean."},
        {"id": 7, "title": "Winter Ledger", "year": 2024, "rating": 8.0,
         "description": "A forensic accountant traces a decades-old fraud through a frozen archive."},
        {"id": 8, "title": "Loud Silence", "year": 2024, "rating": 7.3,
         "description": "A deaf drummer joins an underground band chasing one final show."},
        {"id": 9, "title": "The Amber Line", "year": 2024, "rating": 6.8,
         "description": "Commuters on a stalled train slowly realize the city outside has changed."},
        {"id": 10, "title": "Half-Light District", "year": 2023, "rating": 7.5,
         "description": "A street photographer documents a neighborhood that only exists at dusk."},
    ],
}

FEATURED = {
    "id": 0, "title": "Neon Horizon", "year": 2023, "rating": 8.4,
    "description": "A hacker uncovers a conspiracy hidden inside a city-wide neural network, "
                    "racing against time before her own memories are erased.",
}


def MovieCard(movie, on_select):
    return div(class_name="movie-card", on_click=lambda: on_select(movie), key=movie["id"],
        div(class_name="movie-card-poster", movie["title"][0]),
        div(class_name="movie-card-info",
            p(class_name="movie-card-title", movie["title"]),
            p(class_name="movie-card-meta", f"{movie['year']} · {movie['rating']:.1f}"),
        ),
    )


def MovieRow(title, movies, on_select):
    return section(class_name="movie-row",
        h3(class_name="movie-row-title", title),
        div(class_name="movie-row-track",
            *[MovieCard(m, on_select) for m in movies],
        ),
    )


def DetailPanel(movie, on_close):
    return div(class_name="detail-panel",
        button(class_name="detail-close", on_click=lambda: on_close(), "Close"),
        h2(class_name="detail-title", movie["title"]),
        p(class_name="detail-meta", f"{movie['year']} · Rating {movie['rating']:.1f}"),
        p(class_name="detail-description", movie["description"]),
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
            div(class_name="hero-overlay"),
            div(class_name="hero-content",
                h1(class_name="hero-title", FEATURED["title"]),
                p(class_name="hero-meta", f"{FEATURED['year']} · Rating {FEATURED['rating']:.1f}"),
                p(class_name="hero-description", FEATURED["description"]),
                button(class_name="hero-play", on_click=lambda: select_movie(FEATURED), "More Info"),
            ),
        ),
        DetailPanel(selected, close_panel) if selected else None,
        MovieRow("Trending", MOVIES["trending"], select_movie),
        MovieRow("New", MOVIES["new"], select_movie),
    )
```
