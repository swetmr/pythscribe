<!-- exp=baseline-001 task=macro_movie_rows cond=ps sample=4 model=claude-opus-4-8 cost_usd=0.047546500000000005 -->
```python
"use client"

from pyths.react import component, use_state

TRENDING = [
    {"title": "Neon Skyline", "year": 2023, "rating": 8.4,
     "description": "A rogue architect races to rebuild a city that keeps rewriting itself overnight."},
    {"title": "Deep Current", "year": 2021, "rating": 7.9,
     "description": "Two deep-sea divers uncover a signal older than the ocean floor."},
    {"title": "Paper Kingdoms", "year": 2022, "rating": 8.1,
     "description": "A cartographer's forged maps start reshaping the borders they describe."},
    {"title": "Glass Orchard", "year": 2020, "rating": 7.5,
     "description": "In a town where fruit grows transparent, one grower hides a secret harvest."},
    {"title": "Midnight Relay", "year": 2024, "rating": 8.8,
     "description": "A cross-country courier carries a package that must never stop moving."},
]

NEW = [
    {"title": "Ash & Ivy", "year": 2025, "rating": 8.6,
     "description": "Rival botanists thaw a garden buried under a century of volcanic ash."},
    {"title": "The Quiet Frequency", "year": 2025, "rating": 7.7,
     "description": "A night-shift radio host answers a caller who claims to be from tomorrow."},
    {"title": "Saltwater Saints", "year": 2025, "rating": 8.0,
     "description": "A lighthouse keeper shelters strangers washed in by an impossible tide."},
    {"title": "Lantern Season", "year": 2025, "rating": 7.4,
     "description": "A festival town discovers its floating lanterns are answering back."},
    {"title": "Terminal Bloom", "year": 2025, "rating": 8.9,
     "description": "An airport gardener grows a flower that only opens between departures."},
]

FEATURED = {
    "title": "Midnight Relay",
    "year": 2024,
    "rating": 8.8,
    "description": "A cross-country courier discovers the package she carries rewrites the road ahead. To deliver it she must outrun the people erasing her route, one mile at a time.",
}


def stars(rating):
    return f"★ {rating:.1f}"


@component
def MovieBrowser():
    selected, set_selected = use_state(None)

    def card(movie):
        return div(
            key=f"{movie['title']}-{movie['year']}",
            class_name="movie-card",
            on_click=lambda: set_selected(movie),
            div(class_name="movie-card-poster", movie["title"]),
            div(class_name="movie-card-meta",
                span(class_name="movie-card-title", movie["title"]),
                span(class_name="movie-card-sub", f"{movie['year']} · {stars(movie['rating'])}"),
            ),
        )

    def row(name, movies):
        return section(class_name="movie-row",
            h2(class_name="movie-row-title", name),
            div(class_name="movie-row-track",
                *[card(m) for m in movies]),
        )

    detail = None
    if selected is not None:
        detail = div(class_name="detail-panel",
            div(class_name="detail-panel-inner",
                button(class_name="detail-close",
                       on_click=lambda: set_selected(None), "Close"),
                h3(class_name="detail-title", selected["title"]),
                p(class_name="detail-sub",
                  f"{selected['year']} · {stars(selected['rating'])}"),
                p(class_name="detail-description", selected["description"]),
            ),
        )

    return div(class_name="movie-browser",
        section(class_name="hero",
            div(class_name="hero-content",
                h1(class_name="hero-title", FEATURED["title"]),
                p(class_name="hero-meta",
                  f"{FEATURED['year']} · {stars(FEATURED['rating'])}"),
                p(class_name="hero-description", FEATURED["description"]),
                button(class_name="hero-play",
                       on_click=lambda: set_selected(FEATURED), "More Info"),
            ),
        ),
        detail,
        row("Trending", TRENDING),
        row("New", NEW),
    )
```
