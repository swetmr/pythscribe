<!-- exp=macrov2-opus task=macro_movie_rows cond=ps sample=5 model=claude-opus-4-8 cost_usd=0.04938349999999999 -->
```python
"use client"

from pyths.react import component, use_state

TRENDING = [
    {"id": 1, "title": "Neon Horizon", "year": 2023, "rating": 8.4,
     "description": "A rogue pilot races across a fractured sky to outrun a collapsing empire."},
    {"id": 2, "title": "The Quiet Fathom", "year": 2021, "rating": 7.9,
     "description": "Deep beneath the ice, a lone diver uncovers a signal that was never meant to be heard."},
    {"id": 3, "title": "Paper Cities", "year": 2022, "rating": 8.1,
     "description": "Two rival architects fall for the same impossible skyline."},
    {"id": 4, "title": "Ashfall", "year": 2020, "rating": 7.5,
     "description": "When the volcano wakes, a small town learns who it truly is."},
]

NEW = [
    {"id": 5, "title": "Midnight Cartography", "year": 2024, "rating": 8.8,
     "description": "A cartographer maps a city that rearranges itself every night."},
    {"id": 6, "title": "Glasshouse", "year": 2024, "rating": 7.2,
     "description": "A greenhouse on the edge of the world hides one last living thing."},
    {"id": 7, "title": "Tin Soldiers", "year": 2023, "rating": 8.0,
     "description": "Retired agents are pulled back for one final, impossible mission."},
    {"id": 8, "title": "Solaris Drift", "year": 2024, "rating": 9.1,
     "description": "A crew adrift near a dying star must choose survival or truth."},
]

FEATURED = {
    "id": 0,
    "title": "Solaris Drift",
    "year": 2024,
    "rating": 9.1,
    "description": "Stranded in the gravity well of a dying star, the crew of the Meridian "
                   "must decide what — and who — is worth saving before the light goes out.",
}


@component
def MovieCard(movie, on_open):
    return div(class_name="movie-card", on_click=lambda: on_open(movie),
        div(class_name="movie-card-poster",
            span(class_name="movie-card-initial", movie["title"][0])),
        div(class_name="movie-card-meta",
            h3(class_name="movie-card-title", movie["title"]),
            p(class_name="movie-card-sub", f"{movie['year']} · ★ {movie['rating']:.1f}")),
    )


@component
def MovieRow(title, movies, on_open):
    return section(class_name="movie-row",
        h2(class_name="movie-row-title", title),
        div(class_name="movie-row-track",
            *[MovieCard(key=m["id"], movie=m, on_open=on_open) for m in movies]),
    )


@component
def DetailPanel(movie, on_close):
    return div(class_name="detail-panel",
        div(class_name="detail-panel-body",
            div(class_name="detail-panel-header",
                h2(class_name="detail-panel-title", movie["title"]),
                button(class_name="detail-panel-close", on_click=lambda: on_close(), "Close")),
            p(class_name="detail-panel-sub", f"{movie['year']} · ★ {movie['rating']:.1f}"),
            p(class_name="detail-panel-desc", movie["description"]),
        ),
    )


@component
def MovieBrowser():
    selected, set_selected = use_state(None)

    def open_movie(movie):
        set_selected(movie)

    def close():
        set_selected(None)

    return div(class_name="movie-browser",
        section(class_name="hero",
            div(class_name="hero-overlay",
                h1(class_name="hero-title", FEATURED["title"]),
                p(class_name="hero-sub", f"{FEATURED['year']} · ★ {FEATURED['rating']:.1f}"),
                p(class_name="hero-desc", FEATURED["description"]),
                button(class_name="hero-play", on_click=lambda: open_movie(FEATURED), "More Info"),
            ),
        ),
        DetailPanel(movie=selected, on_close=close) if selected else None,
        div(class_name="movie-rows",
            MovieRow(title="Trending", movies=TRENDING, on_open=open_movie),
            MovieRow(title="New", movies=NEW, on_open=open_movie),
        ),
    )
```
