<!-- exp=macrov2-haiku task=macro_movie_rows cond=ps sample=5 model=claude-haiku-4-5-20251001 cost_usd=0.044196400000000004 -->
```python
"use client"

from pyths.react import component, use_state

@component
def MovieBrowser():
    selected_movie, set_selected_movie = use_state(None)
    
    trending_movies = [
        {"id": 1, "title": "Cosmic Edge", "year": 2024, "rating": 8.5, "description": "A thrilling sci-fi adventure across distant galaxies."},
        {"id": 2, "title": "Midnight Echo", "year": 2023, "rating": 7.8, "description": "A mystery thriller that keeps you on the edge of your seat."},
        {"id": 3, "title": "Heartbound", "year": 2024, "rating": 8.1, "description": "A romantic drama about two souls finding each other."},
        {"id": 4, "title": "Shadows of Time", "year": 2023, "rating": 7.9, "description": "A time-bending narrative that challenges reality."},
        {"id": 5, "title": "Silent Witness", "year": 2024, "rating": 8.3, "description": "An intense crime drama with unexpected twists."},
    ]
    
    new_movies = [
        {"id": 6, "title": "Neon Dreams", "year": 2024, "rating": 8.0, "description": "A cyberpunk tale of rebellion and hope."},
        {"id": 7, "title": "Echoes of Home", "year": 2024, "rating": 7.7, "description": "A poignant story about reconnecting with the past."},
        {"id": 8, "title": "Frozen Hearts", "year": 2024, "rating": 8.2, "description": "A winter romance in a remote mountain village."},
        {"id": 9, "title": "Legacy", "year": 2024, "rating": 8.4, "description": "An epic family saga spanning generations."},
        {"id": 10, "title": "Whispers in the Dark", "year": 2024, "rating": 7.6, "description": "A supernatural mystery that chills to the bone."},
    ]
    
    featured = trending_movies[0]
    
    def select_movie(movie):
        set_selected_movie(movie)
    
    def close_detail():
        set_selected_movie(None)
    
    return div(class_name="movie-browser",
        div(class_name="hero-section",
            div(class_name="hero-content",
                h1(class_name="hero-title", featured["title"]),
                p(class_name="hero-year-rating", f"{featured['year']} • Rating: {featured['rating']}/10"),
                p(class_name="hero-description", featured["description"]),
            ),
        ),
        
        div(class_name="content-sections",
            div(class_name="row-section",
                h2(class_name="row-title", "Trending"),
                div(class_name="movie-row",
                    *[div(class_name="movie-card", on_click=lambda: select_movie(m),
                        div(class_name="card-image", style={"background": f"linear-gradient(135deg, hsl({m['id'] * 35}, 70%, 60%}), hsl({m['id'] * 35 + 20}, 70%, 50%})"}),
                        div(class_name="card-info",
                            h3(class_name="card-title", m["title"]),
                            p(class_name="card-meta", f"{m['year']} • {m['rating']}/10"),
                        ),
                    ) for m in trending_movies]
                ),
            ),
            
            div(class_name="row-section",
                h2(class_name="row-title", "New"),
                div(class_name="movie-row",
                    *[div(class_name="movie-card", on_click=lambda: select_movie(m),
                        div(class_name="card-image", style={"background": f"linear-gradient(135deg, hsl({m['id'] * 35}, 70%, 60%}), hsl({m['id'] * 35 + 20}, 70%, 50%})"}),
                        div(class_name="card-info",
                            h3(class_name="card-title", m["title"]),
                            p(class_name="card-meta", f"{m['year']} • {m['rating']}/10"),
                        ),
                    ) for m in new_movies]
                ),
            ),
        ),
        
        (div(class_name="detail-panel-overlay", on_click=lambda: close_detail(),
            div(class_name="detail-panel", on_click=lambda e: e.stopPropagation(),
                button(class_name="close-button", on_click=lambda: close_detail(), "×"),
                h2(class_name="detail-title", selected_movie["title"]),
                p(class_name="detail-meta", f"{selected_movie['year']} • Rating: {selected_movie['rating']}/10"),
                p(class_name="detail-description", selected_movie["description"]),
            ),
        ) if selected_movie else None),
    )
```
