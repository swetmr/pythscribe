<!-- exp=baseline-sonnet task=macro_movie_rows cond=ps sample=2 model=claude-sonnet-5 cost_usd=0.0332256 -->
```python
"use client"

from pyths.react import component, use_state

MOVIES_TRENDING = [
    {"id": 1, "title": "Nebula Drift", "year": 2024, "rating": 8.7,
     "description": "A lone salvager uncovers a derelict ship hiding a secret that could rewrite the history of the outer colonies."},
    {"id": 2, "title": "Iron Horizon", "year": 2023, "rating": 7.9,
     "description": "In a city ruled by machines, a rebel engineer builds an army of reclaimed robots to fight for freedom."},
    {"id": 3, "title": "Crimson Tide Society", "year": 2022, "rating": 8.2,
     "description": "Two rival gangs of divers race to recover a sunken fortune before the tide turns against them both."},
    {"id": 4, "title": "The Glass Orchard", "year": 2021, "rating": 7.4,
     "description": "A reclusive botanist discovers her family's orchard is the last refuge for a vanishing species."},
    {"id": 5, "title": "Static Bloom", "year": 2024, "rating": 8.0,
     "description": "A signal from an abandoned satellite draws a group of hackers into a conspiracy decades in the making."},
]

MOVIES_NEW = [
    {"id": 6, "title": "Midnight Ferry", "year": 2026, "rating": 7.6,
     "description": "Strangers aboard a late-night ferry crossing become entangled in a mystery that unfolds before dawn."},
    {"id": 7, "title": "Paper Lanterns", "year": 2026, "rating": 8.4,
     "description": "A festival of light brings a fractured family back together for one unforgettable night."},
    {"id": 8, "title": "Echo Chamber", "year": 2025, "rating": 7.1,
     "description": "A podcast producer stumbles onto a recording that predicts events before they happen."},
    {"id": 9, "title": "The Last Cartographer", "year": 2025, "rating": 8.9,
     "description": "In a world where maps are currency, one mapmaker sets out to chart the one place left unmapped."},
    {"id": 10, "title": "Velvet Static", "year": 2026, "rating": 7.8,
     "description": "A washed-up musician gets a second chance when an AI collaborator revives his forgotten sound."},
]

FEATURED = {"id": 0, "title": "Nebula Drift", "year": 2024, "rating": 8.7,
    "description": "A lone salvager uncovers a derelict ship hiding a secret that could rewrite the history of the outer colonies. From the makers of Iron Horizon comes a sprawling space epic about survival, trust, and the cost of ambition."}


def movie_card(movie, on_select):
    return div(key=movie["id"], class_name="movie-card", on_click=lambda: on_select(movie),
        div(class_name="movie-card-thumb", movie["title"][0]),
        div(class_name="movie-card-info",
            p(class_name="movie-card-title", movie["title"]),
            p(class_name="movie-card-meta", f"{movie['year']} · {movie['rating']:.1f}"),
        ),
    )


def movie_row(title, movies, on_select):
    return div(class_name="movie-row",
        h3(class_name="movie-row-title", title),
        div(class_name="movie-row-track",
            *[movie_card(m, on_select) for m in movies],
        ),
    )


def detail_panel(movie, on_close):
    return div(class_name="detail-panel",
        button(class_name="detail-close", on_click=lambda: on_close(), "Close"),
        h2(class_name="detail-title", movie["title"]),
        p(class_name="detail-meta", f"{movie['year']} · Rating {movie['rating']:.1f}"),
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
            h1(class_name="hero-title", FEATURED["title"]),
            p(class_name="hero-meta", f"{FEATURED['year']} · Rating {FEATURED['rating']:.1f}"),
            p(class_name="hero-description", FEATURED["description"]),
        ),
        selected is not None and detail_panel(selected, close_panel),
        movie_row("Trending", MOVIES_TRENDING, select_movie),
        movie_row("New", MOVIES_NEW, select_movie),
    )
```
