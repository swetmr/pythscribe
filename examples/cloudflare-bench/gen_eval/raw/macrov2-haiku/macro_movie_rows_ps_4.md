<!-- exp=macrov2-haiku task=macro_movie_rows cond=ps sample=4 model=claude-haiku-4-5-20251001 cost_usd=0.046916400000000004 -->
```python
"use client"

from pyths.react import component, use_state

FEATURED_MOVIE = {
    "id": "featured",
    "title": "Cosmic Horizons",
    "year": 2024,
    "rating": 8.5,
    "description": "A breathtaking journey through space and time, discovering the mysteries of the universe."
}

TRENDING_MOVIES = [
    {"id": "t1", "title": "Echoes of Tomorrow", "year": 2024, "rating": 7.8, "description": "A thrilling sci-fi adventure across distant galaxies."},
    {"id": "t2", "title": "Midnight Chronicles", "year": 2023, "rating": 7.5, "description": "A dark mystery unfolds in the city shadows."},
    {"id": "t3", "title": "The Last Horizon", "year": 2024, "rating": 8.2, "description": "An epic adventure at the edge of civilization."},
    {"id": "t4", "title": "Shadows Rise", "year": 2023, "rating": 7.3, "description": "A supernatural thriller with twists."},
]

NEW_MOVIES = [
    {"id": "n1", "title": "Eternal Dance", "year": 2024, "rating": 8.1, "description": "A romantic drama spanning decades."},
    {"id": "n2", "title": "The Silent Call", "year": 2024, "rating": 7.9, "description": "A mystery adventure in uncharted lands."},
    {"id": "n3", "title": "Neon Nights", "year": 2024, "rating": 8.0, "description": "A cyberpunk saga in a digital world."},
    {"id": "n4", "title": "Lost Souls", "year": 2024, "rating": 7.6, "description": "A haunting drama about redemption."},
]

@component
def MovieCard(movie, on_click):
    return div(class_name="movie-card",
        on_click=on_click,
        div(class_name="movie-poster", "🎬"),
        div(class_name="movie-info",
            h3(movie["title"]),
            p(class_name="year-rating", f"{movie['year']} • {movie['rating']}/10")
        )
    )

@component
def DetailPanel(movie, on_close):
    return div(class_name="detail-overlay",
        on_click=on_close,
        div(class_name="detail-panel",
            on_click=lambda e: e.stopPropagation(),
            button(class_name="close-btn", on_click=on_close, "✕"),
            h2(movie["title"]),
            p(class_name="meta", f"{movie['year']} • Rating: {movie['rating']}/10"),
            p(class_name="description", movie["description"])
        )
    )

@component
def MovieBrowser():
    selected, set_selected = use_state(None)
    
    def open_detail(movie):
        set_selected(movie)
    
    def close_detail():
        set_selected(None)
    
    return div(class_name="movie-browser",
        div(class_name="hero-section",
            div(class_name="hero-content",
                h1(FEATURED_MOVIE["title"]),
                p(class_name="hero-description", FEATURED_MOVIE["description"]),
                p(class_name="hero-meta",
                  f"Released {FEATURED_MOVIE['year']} • Rating {FEATURED_MOVIE['rating']}/10")
            )
        ),
        div(class_name="movie-row",
            h2("Trending"),
            div(class_name="movie-grid",
                *[MovieCard(movie, on_click=lambda: open_detail(movie))
                  for movie in TRENDING_MOVIES]
            )
        ),
        div(class_name="movie-row",
            h2("New"),
            div(class_name="movie-grid",
                *[MovieCard(movie, on_click=lambda: open_detail(movie))
                  for movie in NEW_MOVIES]
            )
        ),
        DetailPanel(selected, close_detail) if selected else None
    )
```
