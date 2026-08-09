<!-- exp=baseline-001 task=macro_movie_rows cond=ps sample=5 model=claude-opus-4-8 cost_usd=0.052171499999999996 -->
```python
"use client"

from pyths.react import component, use_state

TRENDING = [
    {"title": "Neon Skyline", "year": 2021, "rating": 8.4,
     "description": "A rogue architect races to save a city that rebuilds itself every night."},
    {"title": "Deep Current", "year": 2019, "rating": 7.9,
     "description": "Two divers uncover a signal beneath the trench that predates humanity."},
    {"title": "Paper Kingdoms", "year": 2022, "rating": 8.1,
     "description": "A forger of royal documents accidentally crowns himself in a fading empire."},
    {"title": "The Long Frost", "year": 2020, "rating": 7.5,
     "description": "Survivors of an endless winter barter memories for warmth."},
    {"title": "Circuit Bloom", "year": 2023, "rating": 8.7,
     "description": "A gardener teaches a dying AI how to grow something that lasts."},
]

NEW = [
    {"title": "Salt & Static", "year": 2024, "rating": 8.0,
     "description": "A radio host on a coastal island falls for a voice that isn't broadcast anywhere."},
    {"title": "Midnight Ledger", "year": 2024, "rating": 7.7,
     "description": "An accountant discovers her firm launders time instead of money."},
    {"title": "Glasshouse", "year": 2025, "rating": 8.3,
     "description": "A family lives inside a museum exhibit and can't remember buying the tickets."},
    {"title": "Wolf in the Wires", "year": 2025, "rating": 8.6,
     "description": "A network engineer hunts a predator that only exists during outages."},
    {"title": "Ember Route", "year": 2024, "rating": 7.4,
     "description": "A bus driver takes the last passengers out of a city that is quietly on fire."},
]

FEATURED = {
    "title": "Circuit Bloom",
    "year": 2023,
    "rating": 8.7,
    "description": "In a world where machines outlive their makers, a quiet gardener "
                   "teaches a dying AI how to grow something that finally lasts. A luminous, "
                   "aching story about patience, code, and green things.",
}


def format_rating(value):
    return f"{value:.1f}"


@component
def MovieCard(movie, on_select):
    return button(
        class_name="movie-card",
        on_click=lambda: on_select(movie),
        div(class_name="movie-card-poster",
            span(class_name="movie-card-initial", movie["title"][0])),
        div(class_name="movie-card-meta",
            span(class_name="movie-card-title", movie["title"]),
            span(class_name="movie-card-sub",
                 f"{movie['year']} · ★ {format_rating(movie['rating'])}")),
    )


@component
def MovieRow(title, movies, on_select):
    return section(class_name="movie-row",
        h3(class_name="movie-row-title", title),
        div(class_name="movie-row-track",
            *[MovieCard(key=m["title"], movie=m, on_select=on_select) for m in movies]),
    )


@component
def DetailPanel(movie, on_close):
    return div(class_name="detail-panel", role="dialog",
        div(class_name="detail-panel-body",
            div(class_name="detail-panel-header",
                h2(class_name="detail-panel-title", movie["title"]),
                button(class_name="detail-panel-close",
                       on_click=lambda: on_close(), "Close")),
            p(class_name="detail-panel-facts",
              f"{movie['year']} · ★ {format_rating(movie['rating'])}"),
            p(class_name="detail-panel-desc", movie["description"]),
        ),
    )


@component
def MovieBrowser():
    selected, set_selected = use_state(None)

    def select(movie):
        set_selected(movie)

    def close():
        set_selected(None)

    return div(class_name="movie-browser",
        section(class_name="hero",
            div(class_name="hero-content",
                span(class_name="hero-tag", "Featured"),
                h1(class_name="hero-title", FEATURED["title"]),
                p(class_name="hero-facts",
                  f"{FEATURED['year']} · ★ {format_rating(FEATURED['rating'])}"),
                p(class_name="hero-desc", FEATURED["description"]),
                button(class_name="hero-play",
                       on_click=lambda: select(FEATURED), "More Info"),
            ),
        ),
        MovieRow(title="Trending", movies=TRENDING, on_select=select),
        MovieRow(title="New", movies=NEW, on_select=select),
        DetailPanel(movie=selected, on_close=close) if selected else None,
    )
```
