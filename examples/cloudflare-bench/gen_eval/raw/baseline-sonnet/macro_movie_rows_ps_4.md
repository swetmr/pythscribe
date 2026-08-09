<!-- exp=baseline-sonnet task=macro_movie_rows cond=ps sample=4 model=claude-sonnet-5 cost_usd=0.0334506 -->
```python
"use client"

from pyths.react import component, use_state

TRENDING = [
    {"id": 1, "title": "Neon Horizon", "year": 2024, "rating": 8.7,
     "description": "A rogue courier races across a dying megacity to deliver a signal that could end the war."},
    {"id": 2, "title": "Glass Tide", "year": 2023, "rating": 7.9,
     "description": "Two sisters uncover a submerged city beneath their hometown lake."},
    {"id": 3, "title": "Ashfall", "year": 2022, "rating": 8.2,
     "description": "A volcanologist races the clock as a supervolcano threatens the Pacific Northwest."},
    {"id": 4, "title": "Paper Moths", "year": 2024, "rating": 7.4,
     "description": "An origami artist discovers her folded creatures are coming to life at night."},
    {"id": 5, "title": "Last Signal", "year": 2021, "rating": 8.9,
     "description": "The final crew of a deep-space relay station must decide who gets to go home."},
]

NEW_RELEASES = [
    {"id": 6, "title": "Static Bloom", "year": 2026, "rating": 7.1,
     "description": "A radio DJ starts receiving broadcasts from a version of the city that no longer exists."},
    {"id": 7, "title": "Ironroot", "year": 2026, "rating": 8.0,
     "description": "A blacksmith's daughter inherits a forge that can mend more than metal."},
    {"id": 8, "title": "Quiet Static", "year": 2025, "rating": 7.6,
     "description": "A hearing-impaired detective solves crimes by reading vibrations in the city's subway lines."},
    {"id": 9, "title": "Salt & Circuit", "year": 2026, "rating": 7.8,
     "description": "A fisherman and a shipwrecked android build an unlikely friendship on a remote coastline."},
    {"id": 10, "title": "The Long Thaw", "year": 2025, "rating": 8.3,
     "description": "Survivors of a century-long cryo-sleep wake to a world that rebuilt itself without them."},
]

FEATURED = {
    "id": 0,
    "title": "Neon Horizon",
    "year": 2024,
    "rating": 8.7,
    "description": "A rogue courier races across a dying megacity to deliver a signal that could end the war. "
                    "Featuring breathtaking chase sequences and a haunting synth score.",
}


def format_rating(rating):
    return f"{rating:.1f}"


def movie_card(movie, on_select):
    return div(
        key=movie["id"],
        class_name="movie-card",
        on_click=lambda: on_select(movie),
        div(class_name="movie-card-thumb", movie["title"][0]),
        div(class_name="movie-card-info",
            p(class_name="movie-card-title", movie["title"]),
            p(class_name="movie-card-meta", f"{movie['year']} · {format_rating(movie['rating'])}"),
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
        p(class_name="detail-meta", f"{movie['year']} · Rating: {format_rating(movie['rating'])}"),
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
            div(class_name="hero-content",
                h1(class_name="hero-title", FEATURED["title"]),
                p(class_name="hero-meta", f"{FEATURED['year']} · Rating: {format_rating(FEATURED['rating'])}"),
                p(class_name="hero-description", FEATURED["description"]),
            ),
        ),
        detail_panel(selected, close_panel) if selected else None,
        movie_row("Trending", TRENDING, select_movie),
        movie_row("New", NEW_RELEASES, select_movie),
    )
```
