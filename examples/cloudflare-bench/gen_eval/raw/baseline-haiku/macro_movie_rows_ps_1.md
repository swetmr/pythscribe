<!-- exp=baseline-haiku task=macro_movie_rows cond=ps sample=1 model=claude-haiku-4-5-20251001 cost_usd=0.0502583 -->
```python
"use client"

from pyths.react import component, use_state

FEATURED_MOVIE = {
    "id": "featured-1",
    "title": "The Quantum Paradox",
    "year": 2025,
    "rating": 8.7,
    "description": "A mind-bending sci-fi thriller about a physicist who discovers a way to alter the past—but at what cost? Journey through parallel timelines and philosophical dilemmas in this visually stunning masterpiece."
}

TRENDING_MOVIES = [
    {"id": "trend-1", "title": "Neon Nights", "year": 2025, "rating": 8.2, "description": "A stylish noir thriller set in a cyberpunk megacity."},
    {"id": "trend-2", "title": "Last Dance", "year": 2024, "rating": 7.9, "description": "A passionate romance spanning decades and continents."},
    {"id": "trend-3", "title": "The Heist", "year": 2025, "rating": 8.5, "description": "An elaborate high-stakes robbery that defies all odds."},
    {"id": "trend-4", "title": "Echoes", "year": 2024, "rating": 8.1, "description": "A psychological drama about memory and identity."},
]

NEW_MOVIES = [
    {"id": "new-1", "title": "Beneath the Surface", "year": 2025, "rating": 7.6, "description": "A mysterious underwater expedition uncovers ancient secrets."},
    {"id": "new-2", "title": "Starbound", "year": 2025, "rating": 8.0, "description": "Epic space opera exploring humanity's place in the cosmos."},
    {"id": "new-3", "title": "The Fall", "year": 2025, "rating": 7.8, "description": "A gripping thriller about corruption and betrayal."},
    {"id": "new-4", "title": "Redemption", "year": 2025, "rating": 8.3, "description": "A compelling drama of second chances and hope."},
]

def movie_card(movie, on_select):
    return div(
        class_name="movie-card",
        on_click=lambda: on_select(movie["id"]),
        div(class_name="movie-card-poster"),
        div(
            class_name="movie-card-info",
            h4(class_name="movie-card-title", movie["title"]),
            div(
                class_name="movie-card-meta",
                span(class_name="movie-card-year", f"{movie['year']}"),
                span(class_name="movie-card-rating", f"★ {movie['rating']}")
            )
        )
    )

@component
def MovieBrowser():
    selected_id, set_selected_id = use_state(None)
    
    selected_movie = None
    if selected_id:
        all_movies = [FEATURED_MOVIE] + TRENDING_MOVIES + NEW_MOVIES
        for movie in all_movies:
            if movie["id"] == selected_id:
                selected_movie = movie
                break
    
    def close_panel():
        set_selected_id(None)
    
    modal = (
        div(
            class_name="detail-modal-overlay",
            on_click=close_panel,
            div(
                class_name="detail-modal",
                on_click=lambda e: e.stopPropagation(),
                div(
                    class_name="detail-modal-header",
                    button(
                        class_name="detail-close-button",
                        on_click=close_panel,
                        "✕"
                    )
                ),
                div(
                    class_name="detail-modal-content",
                    h2(class_name="detail-title", selected_movie["title"]),
                    p(
                        class_name="detail-meta",
                        f"{selected_movie['year']} • ★ {selected_movie['rating']}/10"
                    ),
                    p(
                        class_name="detail-description",
                        selected_movie["description"]
                    )
                )
            )
        ) if selected_movie else None
    )
    
    return div(
        class_name="movie-browser",
        div(
            class_name="hero-section",
            div(
                class_name="hero-content",
                h1(class_name="hero-title", FEATURED_MOVIE["title"]),
                p(class_name="hero-subtitle", f"{FEATURED_MOVIE['year']} • ★ {FEATURED_MOVIE['rating']}/10"),
                p(class_name="hero-description", FEATURED_MOVIE["description"]),
                button(
                    class_name="hero-button",
                    on_click=lambda: set_selected_id(FEATURED_MOVIE["id"]),
                    "Learn More"
                )
            )
        ),
        div(
            class_name="movie-row",
            h2(class_name="row-title", "Trending Now"),
            div(
                class_name="row-cards",
                *[movie_card(m, set_selected_id) for m in TRENDING_MOVIES]
            )
        ),
        div(
            class_name="movie-row",
            h2(class_name="row-title", "New Releases"),
            div(
                class_name="row-cards",
                *[movie_card(m, set_selected_id) for m in NEW_MOVIES]
            )
        ),
        modal
    )
```
