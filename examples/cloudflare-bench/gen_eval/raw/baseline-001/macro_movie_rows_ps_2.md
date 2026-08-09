<!-- exp=baseline-001 task=macro_movie_rows cond=ps sample=2 model=claude-opus-4-8 cost_usd=0.0500465 -->
```python
"use client"

from pyths.react import component, use_state

TRENDING = [
    {"id": 1, "title": "Neon Horizon", "year": 2023, "rating": 8.4,
     "description": "A rogue pilot races across a collapsing megacity to outrun the syndicate that built her."},
    {"id": 2, "title": "The Quiet Tide", "year": 2021, "rating": 7.9,
     "description": "A fishing town keeps a century-old secret buried beneath its harbor."},
    {"id": 3, "title": "Iron Meridian", "year": 2022, "rating": 8.1,
     "description": "Two estranged engineers reunite to salvage a derailed transcontinental train."},
    {"id": 4, "title": "Paper Moons", "year": 2020, "rating": 7.2,
     "description": "A con artist and a runaway forge tickets to a life neither can afford."},
]

NEW = [
    {"id": 5, "title": "Glass Orchard", "year": 2024, "rating": 8.7,
     "description": "A botanist grows a garden that only blooms in memories she can no longer trust."},
    {"id": 6, "title": "Last Frequency", "year": 2024, "rating": 7.6,
     "description": "A late-night radio host starts receiving broadcasts from a station that burned down decades ago."},
    {"id": 7, "title": "Cobalt Hours", "year": 2025, "rating": 8.0,
     "description": "In a city where time is currency, a courier discovers she's been paid in stolen days."},
    {"id": 8, "title": "Wildfire Sonata", "year": 2025, "rating": 8.3,
     "description": "A deaf pianist composes her final symphony as the hills around her ignite."},
]

FEATURED = {
    "title": "Neon Horizon",
    "year": 2023,
    "rating": 8.4,
    "description": "A rogue pilot races across a collapsing megacity to outrun the syndicate that built her. Equal parts chase thriller and neon-soaked elegy.",
}


def format_rating(rating):
    return f"★ {rating:.1f}"


@component
def MovieCard(movie, on_select):
    return button(class_name="movie-card", on_click=lambda: on_select(movie),
        div(class_name="movie-card-poster",
            span(class_name="movie-card-initial", movie["title"][0])),
        div(class_name="movie-card-info",
            span(class_name="movie-card-title", movie["title"]),
            span(class_name="movie-card-meta",
                f"{movie['year']} · {format_rating(movie['rating'])}")),
    )


@component
def MovieRow(title, movies, on_select):
    return section(class_name="movie-row",
        h2(class_name="movie-row-title", title),
        div(class_name="movie-row-track",
            *[MovieCard(key=m["id"], movie=m, on_select=on_select) for m in movies]),
    )


@component
def DetailPanel(movie, on_close):
    return div(class_name="detail-panel",
        div(class_name="detail-panel-body",
            div(class_name="detail-panel-header",
                h2(class_name="detail-panel-title", movie["title"]),
                button(class_name="detail-panel-close", on_click=lambda: on_close(), "Close")),
            p(class_name="detail-panel-meta",
                f"{movie['year']} · {format_rating(movie['rating'])}"),
            p(class_name="detail-panel-description", movie["description"]),
        ),
    )


@component
def MovieBrowser():
    selected, set_selected = use_state(None)

    def select_movie(movie):
        set_selected(movie)

    def close_panel():
        set_selected(None)

    return div(class_name="movie-browser",
        section(class_name="hero",
            div(class_name="hero-content",
                span(class_name="hero-eyebrow", "Featured"),
                h1(class_name="hero-title", FEATURED["title"]),
                p(class_name="hero-meta",
                    f"{FEATURED['year']} · {format_rating(FEATURED['rating'])}"),
                p(class_name="hero-description", FEATURED["description"]),
            ),
        ),
        DetailPanel(movie=selected, on_close=close_panel) if selected else None,
        MovieRow(title="Trending", movies=TRENDING, on_select=select_movie),
        MovieRow(title="New", movies=NEW, on_select=select_movie),
    )
```
