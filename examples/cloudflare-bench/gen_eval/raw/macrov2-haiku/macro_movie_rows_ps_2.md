<!-- exp=macrov2-haiku task=macro_movie_rows cond=ps sample=2 model=claude-haiku-4-5-20251001 cost_usd=0.039716400000000006 -->
```python
"use client"

from pyths.react import component, use_state

@component
def MovieBrowser():
    # Inline movie data
    trending_movies = [
        {"id": 1, "title": "Cosmic Adventure", "year": 2023, "rating": 8.5, "description": "An epic journey through the stars and beyond."},
        {"id": 2, "title": "Lost in Time", "year": 2024, "rating": 7.9, "description": "A thrilling time-travel mystery that spans decades."},
        {"id": 3, "title": "Digital Dreams", "year": 2023, "rating": 8.2, "description": "Reality blurs in this mind-bending sci-fi thriller."},
        {"id": 4, "title": "Midnight Code", "year": 2024, "rating": 7.6, "description": "A hacker's desperate race against the clock."},
        {"id": 5, "title": "Neon Nights", "year": 2023, "rating": 8.1, "description": "A cyberpunk love story set in a neon-soaked city."},
    ]
    
    new_movies = [
        {"id": 6, "title": "The Last Station", "year": 2024, "rating": 8.3, "description": "A poignant drama about endings and new beginnings."},
        {"id": 7, "title": "Echoes of Tomorrow", "year": 2024, "rating": 7.8, "description": "Discover alternate futures in this time-bending adventure."},
        {"id": 8, "title": "Shadows Dance", "year": 2024, "rating": 8.0, "description": "A mysterious tale woven from secrets and deception."},
        {"id": 9, "title": "Rising Phoenix", "year": 2024, "rating": 8.4, "description": "From the ashes of despair, a new hero emerges."},
        {"id": 10, "title": "Whispers in the Dark", "year": 2024, "rating": 7.7, "description": "Where psychological horror meets supernatural terror."},
    ]
    
    featured = trending_movies[0]
    selected_movie, set_selected_movie = use_state(None)
    
    def on_movie_click(movie):
        set_selected_movie(movie)
    
    def close_detail():
        set_selected_movie(None)
    
    return div(class_name="movie-browser",
        # Hero section
        div(class_name="hero",
            div(class_name="hero-background",
                style={"background_image": f"url('https://via.placeholder.com/1920x600?text={featured['title']}')"}
            ),
            div(class_name="hero-content",
                h1(class_name="hero-title", featured["title"]),
                p(class_name="hero-year", f"{featured['year']}"),
                p(class_name="hero-rating", f"★ {featured['rating']}/10"),
                p(class_name="hero-description", featured["description"]),
            ),
        ),
        
        # Trending row
        div(class_name="section",
            h2(class_name="section-title", "Trending Now"),
            div(class_name="movie-row",
                *[MovieCard(key=movie["id"], movie=movie, on_click=lambda: on_movie_click(movie))
                  for movie in trending_movies]
            ),
        ),
        
        # New releases row
        div(class_name="section",
            h2(class_name="section-title", "New Releases"),
            div(class_name="movie-row",
                *[MovieCard(key=movie["id"], movie=movie, on_click=lambda: on_movie_click(movie))
                  for movie in new_movies]
            ),
        ),
        
        # Detail panel modal
        DetailPanel(movie=selected_movie, on_close=close_detail) if selected_movie else None,
    )

@component
def MovieCard(movie, on_click):
    return div(class_name="movie-card", on_click=on_click,
        div(class_name="movie-poster",
            style={"background_image": f"url('https://via.placeholder.com/200x300?text={movie['title']}')"}
        ),
        div(class_name="movie-info",
            h3(class_name="movie-title", movie["title"]),
            p(class_name="movie-year", str(movie["year"])),
            p(class_name="movie-rating", f"★ {movie['rating']}"),
        ),
    )

@component
def DetailPanel(movie, on_close):
    return div(class_name="detail-overlay", on_click=on_close,
        div(class_name="detail-panel", on_click=lambda e: e.stopPropagation(),
            div(class_name="detail-header",
                h2(class_name="detail-title", movie["title"]),
                button(class_name="detail-close", on_click=on_close, "✕"),
            ),
            div(class_name="detail-body",
                p(class_name="detail-meta",
                    span("Year: "), span(str(movie["year"])), span(" | "),
                    span("Rating: "), span(f"{movie['rating']}/10"),
                ),
                p(class_name="detail-description", movie["description"]),
            ),
        ),
    )
```
