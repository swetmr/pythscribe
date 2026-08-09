<!-- exp=macrov2-sonnet task=macro_movie_rows cond=ps sample=3 model=claude-sonnet-5 cost_usd=0.0359703 -->
```python
"use client"

from pyths.react import component, use_state

TRENDING = [
    {"id": 1, "title": "Neon Horizon", "year": 2024, "rating": 8.7,
     "description": "A rogue pilot races across a collapsing sky-city to stop a war before it starts."},
    {"id": 2, "title": "The Last Signal", "year": 2023, "rating": 7.9,
     "description": "A radio operator on a dying space station picks up a message from Earth's past."},
    {"id": 3, "title": "Glass Kingdom", "year": 2022, "rating": 8.1,
     "description": "Two rival dynasties battle for control of a city built entirely of glass."},
    {"id": 4, "title": "Midnight Choir", "year": 2024, "rating": 7.5,
     "description": "A disbanded band reunites for one final concert that could change everything."},
    {"id": 5, "title": "Iron Tide", "year": 2021, "rating": 8.3,
     "description": "A salvage crew uncovers a secret buried beneath the ocean floor."},
]

NEW_RELEASES = [
    {"id": 6, "title": "Paper Moons", "year": 2026, "rating": 7.2,
     "description": "A letter carrier discovers messages that predict the future, one town at a time."},
    {"id": 7, "title": "Static Bloom", "year": 2026, "rating": 8.0,
     "description": "In a world where flowers broadcast memories, a botanist chases a forgotten one."},
    {"id": 8, "title": "Hollow Frequency", "year": 2025, "rating": 6.9,
     "description": "A sound engineer records a frequency that no one else can hear, or explain."},
    {"id": 9, "title": "Velvet Circuit", "year": 2026, "rating": 7.8,
     "description": "An underground racer teams up with a rogue AI to outrun the city's control grid."},
    {"id": 10, "title": "Ashfall", "year": 2025, "rating": 8.5,
     "description": "Survivors of a volcanic winter must decide who gets a seat on the last train south."},
]

FEATURED = {
    "id": 0, "title": "Crimson Ascent", "year": 2026, "rating": 9.1,
    "description": "A disgraced climber attempts the impossible summit that claimed her mentor's life, "
                    "racing a storm that could bury the truth forever.",
}


@component
def MovieCard(movie, on_select):
    return div(
        class_name="movie-card",
        on_click=lambda: on_select(movie),
        div(class_name="movie-card-poster", f"{movie['title'][0]}"),
        div(class_name="movie-card-info",
            h4(class_name="movie-card-title", movie["title"]),
            p(class_name="movie-card-meta", f"{movie['year']} • ★ {movie['rating']}"),
        ),
    )


@component
def MovieRow(title, movies, on_select):
    return section(
        class_name="movie-row",
        h3(class_name="movie-row-title", title),
        div(class_name="movie-row-track",
            *[MovieCard(movie=m, on_select=on_select) for m in movies]),
    )


@component
def DetailPanel(movie, on_close):
    return div(
        class_name="detail-panel",
        div(class_name="detail-panel-header",
            h2(class_name="detail-panel-title", movie["title"]),
            button(class_name="detail-panel-close", on_click=lambda: on_close(), "Close"),
        ),
        p(class_name="detail-panel-meta", f"{movie['year']} • ★ {movie['rating']}"),
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
        div(
            class_name="hero",
            div(class_name="hero-content",
                h1(class_name="hero-title", FEATURED["title"]),
                p(class_name="hero-meta", f"{FEATURED['year']} • ★ {FEATURED['rating']}"),
                p(class_name="hero-description", FEATURED["description"]),
                button(class_name="hero-play", on_click=lambda: select_movie(FEATURED), "More Info"),
            ),
        ),
        DetailPanel(movie=selected, on_close=close_panel) if selected else None,
        MovieRow(title="Trending", movies=TRENDING, on_select=select_movie),
        MovieRow(title="New", movies=NEW_RELEASES, on_select=select_movie),
    )
```
