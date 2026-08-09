<!-- exp=macrov2-sonnet task=macro_movie_rows cond=ps sample=4 model=claude-sonnet-5 cost_usd=0.0345903 -->
```python
"use client"

from pyths.react import component, use_state

MOVIES_TRENDING = [
    {"id": 1, "title": "Neon Horizon", "year": 2024, "rating": 8.4,
     "description": "A rogue courier races across a dying city to deliver a secret that could end the war."},
    {"id": 2, "title": "Glass Tide", "year": 2023, "rating": 7.9,
     "description": "When the ocean starts rising overnight, a marine biologist uncovers a conspiracy beneath the waves."},
    {"id": 3, "title": "The Last Ember", "year": 2022, "rating": 8.1,
     "description": "In a frozen world, the keeper of the final flame must decide who deserves its warmth."},
    {"id": 4, "title": "Static Bloom", "year": 2024, "rating": 7.6,
     "description": "A hacker collective discovers that the city's ad networks are broadcasting more than ads."},
    {"id": 5, "title": "Paper Kingdoms", "year": 2021, "rating": 8.7,
     "description": "Two rival origami artists build an empire of folded worlds, one crease at a time."},
]

MOVIES_NEW = [
    {"id": 6, "title": "Midnight Cartographer", "year": 2026, "rating": 7.3,
     "description": "A mapmaker who can chart dreams is hired to find a city that vanished from memory."},
    {"id": 7, "title": "Six Feet of Static", "year": 2026, "rating": 6.9,
     "description": "A late-night radio host starts receiving calls from listeners who haven't been born yet."},
    {"id": 8, "title": "Copperlight", "year": 2025, "rating": 8.0,
     "description": "In a town powered by a single failing generator, a young engineer bets everything on one last fix."},
    {"id": 9, "title": "The Quiet Algorithm", "year": 2025, "rating": 7.8,
     "description": "An AI trained to predict grief starts predicting its own."},
    {"id": 10, "title": "Salt and Circuit", "year": 2026, "rating": 7.1,
     "description": "A smuggler ferries banned technology across a border that exists only on old maps."},
]

FEATURED = {
    "id": 0, "title": "Horizon Protocol", "year": 2026, "rating": 8.9,
    "description": "When a satellite network gains sentience, the engineer who built it is the only one it will listen to.",
}


def movie_card(movie, on_select):
    return div(
        class_name="movie-card",
        key=movie["id"],
        on_click=lambda: on_select(movie),
        div(class_name="movie-card-poster", movie["title"][0]),
        div(class_name="movie-card-info",
            p(class_name="movie-card-title", movie["title"]),
            p(class_name="movie-card-meta", f"{movie['year']} · ★ {movie['rating']}"),
        ),
    )


def movie_row(title, movies, on_select):
    return section(
        class_name="movie-row",
        h3(class_name="movie-row-title", title),
        div(class_name="movie-row-track",
            *[movie_card(m, on_select) for m in movies],
        ),
    )


def detail_panel(movie, on_close):
    return div(
        class_name="detail-panel",
        div(class_name="detail-panel-header",
            h2(movie["title"]),
            button(class_name="detail-panel-close", on_click=lambda: on_close(), "Close"),
        ),
        p(class_name="detail-panel-meta", f"{movie['year']} · ★ {movie['rating']}"),
        p(class_name="detail-panel-description", movie["description"]),
    )


@component
def MovieBrowser():
    selected, set_selected = use_state(None)

    def select_movie(movie):
        set_selected(movie)

    def close_panel():
        set_selected(None)

    return div(
        class_name="movie-browser",
        div(class_name="hero",
            div(class_name="hero-content",
                h1(class_name="hero-title", FEATURED["title"]),
                p(class_name="hero-meta", f"{FEATURED['year']} · ★ {FEATURED['rating']}"),
                p(class_name="hero-description", FEATURED["description"]),
            ),
        ),
        detail_panel(selected, close_panel) if selected else None,
        movie_row("Trending", MOVIES_TRENDING, select_movie),
        movie_row("New", MOVIES_NEW, select_movie),
    )
```
