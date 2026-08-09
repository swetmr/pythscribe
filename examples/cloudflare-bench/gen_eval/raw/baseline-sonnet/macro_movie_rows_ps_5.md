<!-- exp=baseline-sonnet task=macro_movie_rows cond=ps sample=5 model=claude-sonnet-5 cost_usd=0.0334506 -->
```python
"use client"

from pyths.react import component, use_state

MOVIES_TRENDING = [
    {"id": 1, "title": "Nebula Drift", "year": 2024, "rating": 8.7,
     "description": "A crew of salvagers stumbles onto a derelict ship carrying a secret that could rewrite the galaxy's balance of power."},
    {"id": 2, "title": "Glass Horizon", "year": 2023, "rating": 7.9,
     "description": "In a floating city above the clouds, a young engineer uncovers a conspiracy threatening everyone below."},
    {"id": 3, "title": "Red Static", "year": 2025, "rating": 8.2,
     "description": "A detective haunted by a case gone cold reopens the file after a signal from the past resurfaces."},
    {"id": 4, "title": "Paper Tigers", "year": 2022, "rating": 7.4,
     "description": "Two rival startup founders are forced into an uneasy alliance to save their companies from collapse."},
    {"id": 5, "title": "The Long Static", "year": 2024, "rating": 8.9,
     "description": "A radio operator stationed at the edge of the world picks up a transmission that shouldn't exist."},
]

MOVIES_NEW = [
    {"id": 6, "title": "Midnight Ferry", "year": 2026, "rating": 7.6,
     "description": "Strangers aboard a night ferry are drawn together when the boat loses contact with the shore."},
    {"id": 7, "title": "Ash and Ember", "year": 2026, "rating": 8.0,
     "description": "A wildfire season forces a fractured family to confront the choices that scattered them years ago."},
    {"id": 8, "title": "Silent Orbit", "year": 2026, "rating": 7.8,
     "description": "The last astronaut aboard a dying space station races to send a warning before contact is lost."},
    {"id": 9, "title": "Copper Skies", "year": 2025, "rating": 8.3,
     "description": "A traveling mechanic drifts from town to town in a drought-stricken future, fixing machines and mending trust."},
    {"id": 10, "title": "Low Tide", "year": 2026, "rating": 7.2,
     "description": "A coastal town's summer turns strange after a storm washes something ashore that no one can explain."},
]

FEATURED = {
    "id": 0, "title": "Nebula Drift", "year": 2024, "rating": 8.7,
    "description": "A crew of salvagers stumbles onto a derelict ship carrying a secret that could rewrite the galaxy's balance of power. From the makers of Red Static.",
}


def MovieCard(movie, on_select):
    return div(
        key=movie["id"],
        class_name="movie-card",
        on_click=lambda: on_select(movie),
        div(class_name="movie-card-poster", movie["title"][0]),
        div(class_name="movie-card-info",
            p(class_name="movie-card-title", movie["title"]),
            p(class_name="movie-card-meta", f"{movie['year']} · ★ {movie['rating']:.1f}"),
        ),
    )


def MovieRow(title, movies, on_select):
    return div(
        class_name="movie-row",
        h3(class_name="movie-row-title", title),
        div(class_name="movie-row-track",
            *[MovieCard(m, on_select) for m in movies],
        ),
    )


def DetailPanel(movie, on_close):
    return div(
        class_name="detail-panel",
        button(class_name="detail-close", on_click=lambda: on_close(), "Close"),
        h2(class_name="detail-title", movie["title"]),
        p(class_name="detail-meta", f"{movie['year']} · ★ {movie['rating']:.1f}"),
        p(class_name="detail-description", movie["description"]),
    )


@component
def MovieBrowser():
    selected, set_selected = use_state(None)

    def select_movie(movie):
        set_selected(movie)

    def close_panel():
        set_selected(None)

    return div(
        class_name="movie-browser",
        div(
            class_name="hero",
            div(class_name="hero-content",
                h1(class_name="hero-title", FEATURED["title"]),
                p(class_name="hero-meta", f"{FEATURED['year']} · ★ {FEATURED['rating']:.1f}"),
                p(class_name="hero-description", FEATURED["description"]),
            ),
        ),
        selected and DetailPanel(selected, close_panel),
        MovieRow("Trending", MOVIES_TRENDING, select_movie),
        MovieRow("New", MOVIES_NEW, select_movie),
    )
```
