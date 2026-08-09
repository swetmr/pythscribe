<!-- exp=baseline-001 task=macro_movie_rows cond=ps sample=1 model=claude-opus-4-8 cost_usd=0.08384549999999999 -->
```python
"use client"

from pyths.react import component, use_state

TRENDING = [
    {"title": "Echoes of Tomorrow", "year": 2021, "rating": 8.4,
     "description": "A physicist discovers a signal from her future self and must decide whether to trust it."},
    {"title": "The Last Cartographer", "year": 2019, "rating": 7.9,
     "description": "In a world where maps rewrite themselves, one woman charts the final unknown coast."},
    {"title": "Neon Harvest", "year": 2022, "rating": 8.1,
     "description": "Farmers on a rain-soaked megacity rooftop fight a corporation for their glowing crops."},
    {"title": "Silent Orbit", "year": 2020, "rating": 7.5,
     "description": "A lone repair crew aboard a decaying space station races a failing life-support clock."},
    {"title": "Paper Lanterns", "year": 2018, "rating": 8.7,
     "description": "Two rival festival makers rediscover an old friendship over a single summer night."},
]

NEW = [
    {"title": "Quantum Bakery", "year": 2024, "rating": 7.2,
     "description": "A pastry chef accidentally proofs dough across parallel timelines."},
    {"title": "Ashfall Country", "year": 2024, "rating": 8.0,
     "description": "A ranger family holds their valley together as a distant volcano reshapes the sky."},
    {"title": "The Ninth Frequency", "year": 2023, "rating": 7.8,
     "description": "A late-night radio host starts receiving calls from listeners who don't exist yet."},
    {"title": "Glasswing", "year": 2025, "rating": 8.3,
     "description": "A young glassblower inherits a workshop whose creations remember the dead."},
    {"title": "Downriver Blues", "year": 2023, "rating": 7.6,
     "description": "Three siblings float their late father's boat one last time down a vanishing river."},
]

FEATURED = {
    "title": "Echoes of Tomorrow",
    "year": 2021,
    "rating": 8.4,
    "description": "A physicist discovers a signal from her future self and must decide whether to trust it as the fabric of her ordinary life begins to unravel.",
}


def stars(rating):
    filled = round(rating / 2)
    return "".join(["★" if i < filled else "☆" for i in range(5)])


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
                span(class_name="movie-card-year", str(movie["year"])),
            ),
        )

    def row(label, movies):
        return section(class_name="movie-row",
            h2(class_name="movie-row-title", label),
            div(class_name="movie-row-track",
                *[card(m) for m in movies],
            ),
        )

    detail = None
    if selected is not None:
        detail = div(class_name="detail-panel",
            div(class_name="detail-card",
                button(class_name="detail-close",
                       on_click=lambda: set_selected(None), "Close"),
                h2(class_name="detail-title", selected["title"]),
                div(class_name="detail-sub",
                    span(class_name="detail-year", str(selected["year"])),
                    span(class_name="detail-rating",
                         f"{stars(selected['rating'])} {selected['rating']:.1f}"),
                ),
                p(class_name="detail-desc", selected["description"]),
            ),
        )

    return div(class_name="movie-browser",
        section(class_name="hero",
            div(class_name="hero-backdrop"),
            div(class_name="hero-content",
                span(class_name="hero-badge", "Featured"),
                h1(class_name="hero-title", FEATURED["title"]),
                div(class_name="hero-meta",
                    span(class_name="hero-year", str(FEATURED["year"])),
                    span(class_name="hero-rating",
                         f"{stars(FEATURED['rating'])} {FEATURED['rating']:.1f}"),
                ),
                p(class_name="hero-desc", FEATURED["description"]),
                button(class_name="hero-play",
                       on_click=lambda: set_selected(FEATURED), "More Info"),
            ),
        ),
        row("Trending", TRENDING),
        row("New", NEW),
        detail,
    )
```
