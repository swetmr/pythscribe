<!-- exp=macrov2-opus task=macro_movie_rows cond=ps sample=1 model=claude-opus-4-8 cost_usd=0.081994 -->
```python
"use client"

from pyths.react import component, use_state

TRENDING = [
    {"title": "Neon Horizon", "year": 2021, "rating": 8.4,
     "description": "A rogue pilot races across a collapsing skyline to outrun her past."},
    {"title": "The Quiet Depths", "year": 2019, "rating": 7.9,
     "description": "Marine biologists uncover a signal older than the ocean itself."},
    {"title": "Paper Cities", "year": 2022, "rating": 8.1,
     "description": "An architect discovers her blueprints are rewriting the real world."},
    {"title": "Ashfall", "year": 2020, "rating": 7.5,
     "description": "A small town holds its breath as the mountain begins to wake."},
]

NEW = [
    {"title": "Midnight Cartography", "year": 2024, "rating": 8.8,
     "description": "Two strangers map a city that only exists after dark."},
    {"title": "Glasshouse", "year": 2024, "rating": 7.2,
     "description": "A family reunion turns tense inside a home with no secrets left."},
    {"title": "Solar Wake", "year": 2023, "rating": 8.6,
     "description": "The first crew to touch the sun's edge sends home one final message."},
    {"title": "Understudy", "year": 2025, "rating": 7.8,
     "description": "A stand-in actress starts living the roles she was never meant to play."},
]

FEATURED = {
    "title": "Neon Horizon",
    "year": 2021,
    "rating": 8.4,
    "description": "A rogue pilot races across a collapsing skyline to outrun her past, "
                   "chasing a signal that promises the one thing she left behind.",
}


@component
def MovieCard(movie, on_open):
    return button(class_name="movie-card", on_click=lambda: on_open(movie),
        div(class_name="movie-card-poster",
            span(class_name="movie-card-initial", movie["title"][0])),
        div(class_name="movie-card-meta",
            span(class_name="movie-card-title", movie["title"]),
            span(class_name="movie-card-sub", f"{movie['year']} · ★ {movie['rating']:.1f}")),
    )


@component
def MovieRow(title, movies, on_open):
    return section(class_name="movie-row",
        h3(class_name="movie-row-title", title),
        div(class_name="movie-row-track",
            *[MovieCard(key=m["title"], movie=m, on_open=on_open) for m in movies]),
    )


@component
def DetailPanel(movie, on_close):
    return div(class_name="detail-panel",
        div(class_name="detail-panel-body",
            div(class_name="detail-panel-header",
                h2(class_name="detail-title", movie["title"]),
                button(class_name="detail-close", on_click=lambda: on_close(), "Close")),
            p(class_name="detail-sub", f"{movie['year']} · ★ {movie['rating']:.1f}"),
            p(class_name="detail-description", movie["description"]),
        ),
    )


@component
def MovieBrowser():
    selected, set_selected = use_state(None)

    def open_movie(movie):
        set_selected(movie)

    def close_panel():
        set_selected(None)

    return div(class_name="movie-browser",
        section(class_name="hero",
            div(class_name="hero-content",
                h1(class_name="hero-title", FEATURED["title"]),
                p(class_name="hero-sub", f"{FEATURED['year']} · ★ {FEATURED['rating']:.1f}"),
                p(class_name="hero-description", FEATURED["description"]),
                button(class_name="hero-play",
                       on_click=lambda: open_movie(FEATURED), "More Info"),
            ),
        ),
        DetailPanel(movie=selected, on_close=close_panel) if selected else None,
        div(class_name="movie-rows",
            MovieRow(title="Trending", movies=TRENDING, on_open=open_movie),
            MovieRow(title="New", movies=NEW, on_open=open_movie),
        ),
    )
```
