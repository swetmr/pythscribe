<!-- exp=macrov2-opus task=macro_movie_rows cond=ps sample=4 model=claude-opus-4-8 cost_usd=0.05465849999999999 -->
```python
"use client"

from pyths.react import component, use_state

TRENDING = [
    {"title": "Neon Horizon", "year": 2021, "rating": 8.4,
     "description": "A rogue pilot races across a collapsing megacity to outrun her own memories."},
    {"title": "The Silent Orbit", "year": 2019, "rating": 7.9,
     "description": "Stranded astronauts uncover a signal that rewrites what they know about home."},
    {"title": "Copperline", "year": 2022, "rating": 8.1,
     "description": "Two rival detectives chase a forger through a city built on secrets."},
    {"title": "Wildfire Season", "year": 2020, "rating": 7.5,
     "description": "A smokejumper confronts the fire that took everything the summer before."},
    {"title": "Paper Moons", "year": 2023, "rating": 8.7,
     "description": "A con artist and her estranged daughter pull one last impossible heist."},
]

NEW = [
    {"title": "Glasswater", "year": 2024, "rating": 8.2,
     "description": "A marine biologist hears whales singing a language no one has ever recorded."},
    {"title": "Ironclad Hearts", "year": 2024, "rating": 7.7,
     "description": "In a rusting shipyard town, two welders build something the whole city fears."},
    {"title": "The Long Noon", "year": 2025, "rating": 8.9,
     "description": "A drifter arrives in a border town where the sun refuses to set."},
    {"title": "Static Bloom", "year": 2024, "rating": 7.3,
     "description": "A radio host starts receiving broadcasts from a station that burned down decades ago."},
    {"title": "Undertow", "year": 2025, "rating": 8.0,
     "description": "A lifeguard's quiet season unravels when the tide keeps returning the same body."},
]

FEATURED = {
    "title": "Neon Horizon",
    "year": 2021,
    "rating": 8.4,
    "description": "In a city that never powers down, a rogue pilot races the sunrise to outrun her own past. A propulsive, neon-soaked thriller about freedom, memory, and the roads we can't stop driving.",
}


@component
def MovieCard(movie, on_open):
    return div(class_name="movie-card", on_click=lambda: on_open(movie),
        div(class_name="movie-card-poster",
            span(class_name="movie-card-rating", f"★ {movie['rating']:.1f}"),
        ),
        div(class_name="movie-card-meta",
            span(class_name="movie-card-title", movie["title"]),
            span(class_name="movie-card-year", str(movie["year"])),
        ),
    )


@component
def MovieRow(title, movies, on_open):
    return section(class_name="movie-row",
        h3(class_name="movie-row-title", title),
        div(class_name="movie-row-track",
            *[MovieCard(key=m["title"], movie=m, on_open=on_open) for m in movies],
        ),
    )


@component
def DetailPanel(movie, on_close):
    return div(class_name="detail-panel",
        div(class_name="detail-panel-body",
            div(class_name="detail-panel-header",
                h2(class_name="detail-panel-title", movie["title"]),
                button(class_name="detail-panel-close", on_click=lambda: on_close(), "Close"),
            ),
            div(class_name="detail-panel-facts",
                span(class_name="detail-panel-year", str(movie["year"])),
                span(class_name="detail-panel-rating", f"★ {movie['rating']:.1f}"),
            ),
            p(class_name="detail-panel-description", movie["description"]),
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
                span(class_name="hero-badge", "Featured"),
                h1(class_name="hero-title", FEATURED["title"]),
                div(class_name="hero-facts",
                    span(class_name="hero-year", str(FEATURED["year"])),
                    span(class_name="hero-rating", f"★ {FEATURED['rating']:.1f}"),
                ),
                p(class_name="hero-description", FEATURED["description"]),
                button(class_name="hero-play",
                       on_click=lambda: open_movie(FEATURED), "More Info"),
            ),
        ),
        MovieRow(title="Trending", movies=TRENDING, on_open=open_movie),
        MovieRow(title="New", movies=NEW, on_open=open_movie),
        DetailPanel(movie=selected, on_close=close_panel) if selected else None,
    )
```
