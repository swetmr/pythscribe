<!-- exp=baseline-sonnet task=macro_movie_rows cond=ps sample=1 model=claude-sonnet-5 cost_usd=0.055035 -->
```python
"use client"

from pyths.react import component, use_state

MOVIES = {
    "trending": [
        {"id": 1, "title": "Neon Horizon", "year": 2024, "rating": 8.4,
         "description": "A rogue hacker uncovers a conspiracy that spans the last stretch of a dying megacity."},
        {"id": 2, "title": "Silent Orbit", "year": 2023, "rating": 7.9,
         "description": "Stranded on a derelict space station, a crew must decide who gets the last escape pod."},
        {"id": 3, "title": "Glass Kingdom", "year": 2022, "rating": 8.1,
         "description": "Two rival dynasties fight for control of a floating city built entirely of glass."},
        {"id": 4, "title": "Wolf Hour", "year": 2024, "rating": 7.6,
         "description": "A detective haunted by her past hunts a killer who only strikes at 4 a.m."},
        {"id": 5, "title": "Paper Tigers", "year": 2021, "rating": 8.7,
         "description": "Three estranged siblings reunite to save their late father's failing paper mill."},
    ],
    "new": [
        {"id": 6, "title": "Static Bloom", "year": 2026, "rating": 7.2,
         "description": "A botanist discovers a flower that grows only in the presence of electromagnetic noise."},
        {"id": 7, "title": "Last Light Diner", "year": 2026, "rating": 8.0,
         "description": "Strangers trapped in a roadside diner during a blackout confront long-buried secrets."},
        {"id": 8, "title": "Fathom", "year": 2025, "rating": 7.5,
         "description": "A deep-sea salvage team finds something that was never meant to surface."},
        {"id": 9, "title": "Ironwood", "year": 2025, "rating": 8.3,
         "description": "A retired boxer trains one last fighter to save his crumbling gym from demolition."},
        {"id": 10, "title": "Quiet Static", "year": 2026, "rating": 7.8,
         "description": "A radio operator in a remote outpost begins receiving messages from a station that no longer exists."},
    ],
}

FEATURED = {
    "id": 0, "title": "Neon Horizon", "year": 2024, "rating": 8.4,
    "description": "A rogue hacker uncovers a conspiracy that spans the last stretch of a dying megacity, "
                    "racing against a corporate militia before the city's last power grid goes dark.",
}


def MovieCard(movie, on_select):
    return div(
        class_name="movie-card",
        key=movie["id"],
        on_click=lambda: on_select(movie),
        div(class_name="movie-card-title", movie["title"]),
        div(class_name="movie-card-meta", f"{movie['year']} · ★ {movie['rating']:.1f}"),
    )


def MovieRow(title, movies, on_select):
    return div(
        class_name="movie-row",
        h3(class_name="movie-row-title", title),
        div(
            class_name="movie-row-track",
            *[MovieCard(m, on_select) for m in movies],
        ),
    )


def DetailPanel(movie, on_close):
    return div(
        class_name="detail-panel",
        button(class_name="detail-close", on_click=on_close, "Close"),
        h2(class_name="detail-title", movie["title"]),
        div(class_name="detail-meta", f"{movie['year']} · ★ {movie['rating']:.1f}"),
        p(class_name="detail-description", movie["description"]),
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
            h1(class_name="hero-title", FEATURED["title"]),
            div(class_name="hero-meta", f"{FEATURED['year']} · ★ {FEATURED['rating']:.1f}"),
            p(class_name="hero-description", FEATURED["description"]),
        ),
        DetailPanel(selected, close_panel) if selected else None,
        MovieRow("Trending", MOVIES["trending"], select_movie),
        MovieRow("New", MOVIES["new"], select_movie),
    )
```
