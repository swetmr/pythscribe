<!-- exp=baseline-haiku task=macro_movie_rows cond=ps sample=4 model=claude-haiku-4-5-20251001 cost_usd=0.05354420000000001 -->
```python
"use client"

from pyths.react import component, use_state

FEATURED_MOVIE = {
    "id": "featured-1",
    "title": "The Cosmic Adventure",
    "year": 2024,
    "rating": 8.5,
    "description": "A breathtaking journey through space and time as a lone astronaut discovers the secrets of the universe."
}

TRENDING_MOVIES = [
    {"id": "t1", "title": "Echoes of Tomorrow", "year": 2024, "rating": 8.2, "description": "In a world where memories can be stolen, one detective must solve the ultimate mystery."},
    {"id": "t2", "title": "The Last Protocol", "year": 2023, "rating": 7.9, "description": "A hacker discovers a government conspiracy that could change everything."},
    {"id": "t3", "title": "Neon Dreams", "year": 2024, "rating": 8.0, "description": "Two rival musicians must work together to save their city from chaos."},
    {"id": "t4", "title": "Quantum Leap", "year": 2023, "rating": 7.8, "description": "Scientists accidentally open a portal to an alternate dimension."},
]

NEW_MOVIES = [
    {"id": "n1", "title": "Midnight Confessions", "year": 2025, "rating": 7.5, "description": "An intimate drama about second chances and redemption."},
    {"id": "n2", "title": "The Forgotten City", "year": 2025, "rating": 8.1, "description": "Archaeologists uncover an ancient civilization with modern technology."},
    {"id": "n3", "title": "Shattered Glass", "year": 2025, "rating": 7.7, "description": "A journalist must expose the truth before the powerful silence her."},
    {"id": "n4", "title": "Dancing in the Rain", "year": 2025, "rating": 7.4, "description": "A coming-of-age story set in the vibrant streets of a coastal town."},
]


@component
def MovieCard(movie, on_click):
    return div(class_name="movie-card", on_click=lambda: on_click(movie),
        div(class_name="card-image"),
        div(class_name="card-info",
            h3(class_name="card-title", movie["title"]),
            div(class_name="card-meta",
                span(f"{movie['year']}"),
                span(class_name="rating", f"★ {movie['rating']}")
            )
        )
    )


@component
def DetailPanel(movie, on_close):
    return div(class_name="detail-panel-overlay", on_click=on_close,
        div(class_name="detail-panel", on_click=lambda e: e.stopPropagation(),
            button(class_name="close-btn", on_click=on_close, "✕"),
            div(class_name="detail-content",
                h2(class_name="detail-title", movie["title"]),
                div(class_name="detail-meta",
                    span(f"{movie['year']}"),
                    span(class_name="rating", f"★ {movie['rating']}")
                ),
                p(class_name="detail-description", movie["description"])
            )
        )
    )


@component
def MovieBrowser():
    selected_movie, set_selected_movie = use_state(None)

    def handle_card_click(movie):
        set_selected_movie(movie)

    def handle_close():
        set_selected_movie(None)

    return div(class_name="movie-browser",
        div(class_name="hero-section",
            div(class_name="hero-image"),
            div(class_name="hero-content",
                h1(class_name="hero-title", FEATURED_MOVIE["title"]),
                p(class_name="hero-description", FEATURED_MOVIE["description"]),
                div(class_name="hero-meta",
                    span(f"Year: {FEATURED_MOVIE['year']}"),
                    span(class_name="rating", f"Rating: ★ {FEATURED_MOVIE['rating']}")
                )
            )
        ),
        div(class_name="browse-section",
            div(class_name="row",
                h2(class_name="row-title", "Trending"),
                div(class_name="row-content",
                    *[MovieCard(movie=movie, on_click=handle_card_click, key=movie["id"]) for movie in TRENDING_MOVIES]
                )
            ),
            div(class_name="row",
                h2(class_name="row-title", "New"),
                div(class_name="row-content",
                    *[MovieCard(movie=movie, on_click=handle_card_click, key=movie["id"]) for movie in NEW_MOVIES]
                )
            )
        ),
        (DetailPanel(movie=selected_movie, on_close=handle_close) if selected_movie else None)
    )
```
