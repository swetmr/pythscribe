<!-- exp=macrov2-sonnet task=macro_movie_rows cond=ps sample=2 model=claude-sonnet-5 cost_usd=0.0358203 -->
```python
"use client"

from pyths.react import component, use_state

MOVIES_TRENDING = [
    {"id": 1, "title": "Neon Horizon", "year": 2023, "rating": 8.4,
     "description": "A rogue courier races across a fractured megacity to deliver a secret that could topple the corporate order."},
    {"id": 2, "title": "Glass Tide", "year": 2021, "rating": 7.6,
     "description": "Two rival fishing towns must unite when an ancient current threatens to swallow the coastline whole."},
    {"id": 3, "title": "Ember & Ash", "year": 2022, "rating": 8.1,
     "description": "A wildfire investigator uncovers a conspiracy buried in the ashes of her hometown."},
    {"id": 4, "title": "Paper Moons", "year": 2020, "rating": 7.2,
     "description": "An origami artist's fragile world unravels when her estranged brother returns with a debt to pay."},
    {"id": 5, "title": "Static Bloom", "year": 2024, "rating": 8.9,
     "description": "In a world where memories can be transplanted, a gardener discovers a stolen past growing in her greenhouse."},
]

MOVIES_NEW = [
    {"id": 6, "title": "Midnight Ledger", "year": 2026, "rating": 7.9,
     "description": "An accountant stumbles onto a decades-old fraud that connects every family in her small town."},
    {"id": 7, "title": "Salt & Static", "year": 2026, "rating": 8.0,
     "description": "A radio operator on a remote island picks up a signal that shouldn't exist."},
    {"id": 8, "title": "Velvet Circuit", "year": 2025, "rating": 7.4,
     "description": "A retired hacker is pulled back into the game when her old crew resurfaces with one last job."},
    {"id": 9, "title": "Hollow Orchard", "year": 2025, "rating": 8.3,
     "description": "A family inherits an orchard that grows fruit tasting of memories long forgotten."},
    {"id": 10, "title": "Iron Lantern", "year": 2026, "rating": 7.1,
     "description": "In a besieged mountain city, a lamplighter becomes the last line of defense against the encroaching dark."},
]

FEATURED = {
    "id": 5, "title": "Static Bloom", "year": 2024, "rating": 8.9,
    "description": "In a world where memories can be transplanted, a gardener discovers a stolen past growing in her greenhouse. Now she must decide whether to return it or let it bloom into something new.",
}


def format_rating(rating):
    return f"{rating:.1f}"


@component
def MovieCard(movie, on_select):
    return div(class_name="movie-card", on_click=lambda: on_select(movie),
        div(class_name="movie-card-poster", movie["title"][0]),
        div(class_name="movie-card-info",
            p(class_name="movie-card-title", movie["title"]),
            p(class_name="movie-card-meta", f"{movie['year']} · ★ {format_rating(movie['rating'])}"),
        ),
    )


@component
def MovieRow(title, movies, on_select):
    return div(class_name="movie-row",
        h3(class_name="movie-row-title", title),
        div(class_name="movie-row-track",
            *[MovieCard(movie=m, on_select=on_select, key=m["id"]) for m in movies],
        ),
    )


@component
def DetailPanel(movie, on_close):
    return div(class_name="detail-panel",
        div(class_name="detail-panel-header",
            h2(class_name="detail-panel-title", movie["title"]),
            button(class_name="detail-panel-close", on_click=lambda: on_close(), "Close"),
        ),
        p(class_name="detail-panel-meta", f"{movie['year']} · ★ {format_rating(movie['rating'])}"),
        p(class_name="detail-panel-description", movie["description"]),
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
            div(class_name="hero-content",
                h1(class_name="hero-title", FEATURED["title"]),
                p(class_name="hero-meta", f"{FEATURED['year']} · ★ {format_rating(FEATURED['rating'])}"),
                p(class_name="hero-description", FEATURED["description"]),
                button(class_name="hero-play", on_click=lambda: select_movie(FEATURED), "More Info"),
            ),
        ),
        DetailPanel(movie=selected, on_close=close_panel) if selected else None,
        MovieRow(title="Trending", movies=MOVIES_TRENDING, on_select=select_movie),
        MovieRow(title="New", movies=MOVIES_NEW, on_select=select_movie),
    )
```
