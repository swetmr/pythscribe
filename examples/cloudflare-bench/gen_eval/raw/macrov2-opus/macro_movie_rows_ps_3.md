<!-- exp=macrov2-opus task=macro_movie_rows cond=ps sample=3 model=claude-opus-4-8 cost_usd=0.05238349999999999 -->
```python
"use client"

from pyths.react import component, use_state

TRENDING = [
    {"title": "Neon Skyline", "year": 2021, "rating": 8.4,
     "description": "A courier races through a rain-soaked megacity to deliver a secret that could topple its rulers."},
    {"title": "Deep Current", "year": 2019, "rating": 7.6,
     "description": "A marine biologist uncovers a hidden ecosystem — and a conspiracy — beneath the ocean floor."},
    {"title": "Paper Kingdom", "year": 2022, "rating": 8.9,
     "description": "Two rival origami masters compete for a legendary title while an old feud resurfaces."},
    {"title": "Static Horizon", "year": 2020, "rating": 7.1,
     "description": "Stranded astronauts pick up a signal that shouldn't exist, drifting far from home."},
    {"title": "Amber Hours", "year": 2018, "rating": 8.0,
     "description": "A small-town clockmaker discovers he can pause time for exactly sixty seconds."},
]

NEW = [
    {"title": "Glass Meridian", "year": 2024, "rating": 9.1,
     "description": "An architect designs a tower that begins to reshape the memories of everyone inside it."},
    {"title": "Wildroot", "year": 2023, "rating": 7.8,
     "description": "A botanist returns to her family orchard and unearths a decades-old secret in the soil."},
    {"title": "Signal Fire", "year": 2024, "rating": 8.3,
     "description": "Two strangers on opposite coasts fall in love through a failing radio frequency."},
    {"title": "The Long Noon", "year": 2023, "rating": 7.4,
     "description": "In a town where the sun never sets, a sheriff hunts a thief who steals shadows."},
    {"title": "Cobalt Lullaby", "year": 2024, "rating": 8.7,
     "description": "A retired singer is pulled back on stage for one last performance that changes everything."},
]

FEATURED = {
    "title": "Paper Kingdom",
    "year": 2022,
    "rating": 8.9,
    "description": "Two rival origami masters compete for a legendary title while an old feud resurfaces — a story of precision, pride, and paper.",
}


@component
def MovieCard(movie, on_select):
    return div(class_name="movie-card", on_click=lambda: on_select(movie),
        div(class_name="movie-card-poster",
            span(class_name="movie-card-initial", movie["title"][0]),
        ),
        div(class_name="movie-card-meta",
            span(class_name="movie-card-title", movie["title"]),
            span(class_name="movie-card-sub", f"{movie['year']} · ★ {movie['rating']:.1f}"),
        ),
    )


@component
def MovieRow(title, movies, on_select):
    return section(class_name="movie-row",
        h3(class_name="movie-row-title", title),
        div(class_name="movie-row-track",
            *[MovieCard(key=m["title"], movie=m, on_select=on_select) for m in movies],
        ),
    )


@component
def DetailPanel(movie, on_close):
    return div(class_name="detail-panel",
        div(class_name="detail-panel-body",
            div(class_name="detail-panel-header",
                h2(class_name="detail-title", movie["title"]),
                button(class_name="detail-close", on_click=lambda: on_close(), "Close"),
            ),
            p(class_name="detail-meta", f"{movie['year']} · ★ {movie['rating']:.1f}"),
            p(class_name="detail-description", movie["description"]),
        ),
    )


@component
def MovieBrowser():
    selected, set_selected = use_state(None)

    return div(class_name="movie-browser",
        section(class_name="hero",
            div(class_name="hero-content",
                span(class_name="hero-eyebrow", "Featured"),
                h1(class_name="hero-title", FEATURED["title"]),
                p(class_name="hero-meta", f"{FEATURED['year']} · ★ {FEATURED['rating']:.1f}"),
                p(class_name="hero-description", FEATURED["description"]),
                button(class_name="hero-play",
                       on_click=lambda: set_selected(FEATURED), "More Info"),
            ),
        ),
        MovieRow(title="Trending", movies=TRENDING, on_select=lambda m: set_selected(m)),
        MovieRow(title="New", movies=NEW, on_select=lambda m: set_selected(m)),
        DetailPanel(movie=selected, on_close=lambda: set_selected(None)) if selected else None,
    )
```
