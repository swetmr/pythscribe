<!-- exp=macrov2-haiku task=macro_movie_rows cond=ps sample=3 model=claude-haiku-4-5-20251001 cost_usd=0.0386014 -->
```python
"use client"

from pyths.react import component, use_state

@component
def MovieBrowser():
    selected_movie, set_selected_movie = use_state(None)
    
    featured = {
        "id": "featured",
        "title": "Cosmic Odyssey",
        "year": 2024,
        "rating": 8.7,
        "description": "An epic journey through space and time, exploring the mysteries of the universe."
    }
    
    trending = [
        {"id": "t1", "title": "Neon Dreams", "year": 2024, "rating": 8.2, "description": "A cyberpunk thriller in a futuristic megacity."},
        {"id": "t2", "title": "Echoes", "year": 2023, "rating": 7.9, "description": "A mysterious sci-fi drama about parallel realities."},
        {"id": "t3", "title": "The Last Signal", "year": 2024, "rating": 8.5, "description": "Humanity's final hope to save Earth."},
        {"id": "t4", "title": "Midnight Run", "year": 2023, "rating": 7.6, "description": "An action-packed heist film."},
        {"id": "t5", "title": "Whispers in the Dark", "year": 2024, "rating": 8.1, "description": "A psychological thriller that will haunt you."},
    ]
    
    new = [
        {"id": "n1", "title": "Quantum Leap", "year": 2024, "rating": 8.3, "description": "Revolutionary technology changes everything."},
        {"id": "n2", "title": "Silent Bloom", "year": 2024, "rating": 7.8, "description": "An intimate drama set in a quiet town."},
        {"id": "n3", "title": "The Forgotten God", "year": 2024, "rating": 8.4, "description": "A mythological adventure unlike any other."},
        {"id": "n4", "title": "Chrome Hearts", "year": 2024, "rating": 7.7, "description": "A romantic sci-fi love story."},
        {"id": "n5", "title": "Frozen Horizon", "year": 2024, "rating": 8.0, "description": "Survival in the harshest environment on Earth."},
    ]
    
    return div(class_name="movie-browser",
        div(class_name="hero-section",
            div(class_name="hero-content",
                h1(featured["title"]),
                p(class_name="hero-meta", f"{featured['year']} • ⭐ {featured['rating']}"),
                p(class_name="hero-description", featured["description"]),
            ),
        ),
        div(class_name="section",
            h2("Trending Now"),
            div(class_name="movie-row",
                *[MovieCard(movie, on_click=lambda: set_selected_movie(movie))
                  for movie in trending],
            ),
        ),
        div(class_name="section",
            h2("New Releases"),
            div(class_name="movie-row",
                *[MovieCard(movie, on_click=lambda: set_selected_movie(movie))
                  for movie in new],
            ),
        ),
        selected_movie and DetailPanel(movie=selected_movie, on_close=lambda: set_selected_movie(None)),
    )

@component
def MovieCard(movie, on_click):
    return div(class_name="movie-card", on_click=on_click,
        div(class_name="card-poster"),
        div(class_name="card-title", movie["title"]),
        div(class_name="card-meta", f"{movie['year']} • ⭐ {movie['rating']}"),
    )

@component
def DetailPanel(movie, on_close):
    return div(class_name="detail-overlay", on_click=on_close,
        div(class_name="detail-panel", on_click=lambda e: e.stopPropagation(),
            button(class_name="close-button", on_click=on_close, "✕"),
            h2(movie["title"]),
            p(class_name="detail-meta", f"{movie['year']} • ⭐ {movie['rating']}"),
            p(class_name="detail-description", movie["description"]),
        ),
    )
```
