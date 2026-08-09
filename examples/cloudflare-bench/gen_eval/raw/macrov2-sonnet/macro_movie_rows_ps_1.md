<!-- exp=macrov2-sonnet task=macro_movie_rows cond=ps sample=1 model=claude-sonnet-5 cost_usd=0.0575016 -->
```python
"use client"

from pyths.react import component, use_state

MOVIES = {
    "trending": [
        {"id": 1, "title": "Neon Horizon", "year": 2024, "rating": 8.4,
         "description": "A hacker uncovers a conspiracy hidden inside a city-wide neural network."},
        {"id": 2, "title": "Glass Tide", "year": 2023, "rating": 7.6,
         "description": "Two rival fishing families navigate a shrinking coastline and old grudges."},
        {"id": 3, "title": "Ashfall", "year": 2022, "rating": 8.9,
         "description": "Survivors of a volcanic winter build a fragile new society underground."},
        {"id": 4, "title": "Paper Wolves", "year": 2021, "rating": 7.1,
         "description": "A disgraced journalist chases one last story that could clear her name."},
        {"id": 5, "title": "Static Bloom", "year": 2024, "rating": 8.0,
         "description": "An artist's dreams start leaking into the waking world of her city."},
    ],
    "new": [
        {"id": 6, "title": "Low Orbit", "year": 2026, "rating": 7.8,
         "description": "A repair crew stranded on a dying space station must choose who survives."},
        {"id": 7, "title": "Salt & Circuit", "year": 2026, "rating": 8.2,
         "description": "A retired android detective is pulled into one final case."},
        {"id": 8, "title": "The Long Thaw", "year": 2025, "rating": 6.9,
         "description": "A small town wakes from decades of frozen isolation to a changed world."},
        {"id": 9, "title": "Ember Road", "year": 2025, "rating": 7.4,
         "description": "Two strangers cross a burning continent in search of the last safe port."},
        {"id": 10, "title": "Quiet Static", "year": 2026, "rating": 8.6,
         "description": "A sound engineer discovers a frequency that lets her hear the past."},
    ],
}

FEATURED = {
    "id": 0, "title": "Ashfall", "year": 2022, "rating": 8.9,
    "description": "Survivors of a volcanic winter build a fragile new society underground, "
                    "while old rivalries threaten to tear it apart from within.",
}


def rating_stars(rating):
    full = round(rating / 2)
    return "★" * full + "☆" * (5 - full)


@component
def MovieCard(movie, on_select):
    return div(class_name="movie-card", on_click=lambda: on_select(movie),
        div(class_name="movie-card-poster", movie["title"][0]),
        div(class_name="movie-card-info",
            p(class_name="movie-card-title", movie["title"]),
            p(class_name="movie-card-meta", f"{movie['year']} · {movie['rating']:.1f}"),
        ),
    )


@component
def MovieRow(title, movies, on_select):
    return section(class_name="movie-row",
        h3(class_name="movie-row-title", title),
        div(class_name="movie-row-track",
            *[MovieCard(key=m["id"], movie=m, on_select=on_select) for m in movies],
        ),
    )


@component
def DetailPanel(movie, on_close):
    return div(class_name="detail-panel",
        div(class_name="detail-panel-header",
            h2(movie["title"]),
            button(class_name="detail-close-btn", on_click=lambda: on_close(), "Close"),
        ),
        p(class_name="detail-meta", f"{movie['year']} · {movie['rating']:.1f} {rating_stars(movie['rating'])}"),
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
                p(class_name="hero-meta", f"{FEATURED['year']} · {FEATURED['rating']:.1f} {rating_stars(FEATURED['rating'])}"),
                p(class_name="hero-description", FEATURED["description"]),
                button(class_name="hero-play-btn", on_click=lambda: select_movie(FEATURED), "More Info"),
            ),
        ),
        DetailPanel(movie=selected, on_close=close_panel) if selected else None,
        div(class_name="rows",
            MovieRow(title="Trending", movies=MOVIES["trending"], on_select=select_movie),
            MovieRow(title="New", movies=MOVIES["new"], on_select=select_movie),
        ),
    )
```
