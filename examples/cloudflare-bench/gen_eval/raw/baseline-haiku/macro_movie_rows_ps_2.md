<!-- exp=baseline-haiku task=macro_movie_rows cond=ps sample=2 model=claude-haiku-4-5-20251001 cost_usd=0.0236442 -->
```python
"use client"

from pyths.react import component, use_state

@component
def MovieBrowser():
    movies_data = [
        {
            "id": 1,
            "title": "Cosmic Journey",
            "year": 2024,
            "rating": 8.5,
            "description": "A stunning sci-fi adventure across distant galaxies.",
            "category": "trending"
        },
        {
            "id": 2,
            "title": "Silent Echo",
            "year": 2023,
            "rating": 7.8,
            "description": "A mysterious thriller with unexpected twists.",
            "category": "trending"
        },
        {
            "id": 3,
            "title": "Heart's Promise",
            "year": 2024,
            "rating": 8.2,
            "description": "A heartwarming romance set in coastal Italy.",
            "category": "trending"
        },
        {
            "id": 4,
            "title": "Neon Nights",
            "year": 2024,
            "rating": 7.9,
            "description": "A cyberpunk action film set in futuristic Tokyo.",
            "category": "new"
        },
        {
            "id": 5,
            "title": "Whispered Secrets",
            "year": 2024,
            "rating": 8.0,
            "description": "A gripping drama about family secrets.",
            "category": "new"
        },
        {
            "id": 6,
            "title": "Enchanted Woods",
            "year": 2024,
            "rating": 8.3,
            "description": "A magical fantasy adventure for all ages.",
            "category": "new"
        },
    ]
    
    selected_movie, set_selected_movie = use_state(None)
    featured = movies_data[0]
    trending = [m for m in movies_data if m["category"] == "trending"]
    new_releases = [m for m in movies_data if m["category"] == "new"]
    
    def open_detail(movie):
        set_selected_movie(movie)
    
    def close_detail():
        set_selected_movie(None)
    
    return div(class_name="movie-browser",
        div(class_name="hero",
            div(class_name="hero-content",
                h1(featured["title"]),
                p(f"Released {featured['year']} • Rating: {featured['rating']}/10"),
                p(featured["description"]),
                button(on_click=lambda: open_detail(featured), class_name="btn-primary", "Watch Now"),
            ),
        ),
        
        section(class_name="row-section",
            h2("Trending"),
            div(class_name="movie-row",
                *[movie_card(m, lambda movie=m: open_detail(movie)) for m in trending],
            ),
        ),
        
        section(class_name="row-section",
            h2("New Releases"),
            div(class_name="movie-row",
                *[movie_card(m, lambda movie=m: open_detail(movie)) for m in new_releases],
            ),
        ),
        
        (detail_panel(selected_movie, close_detail) if selected_movie else None),
    )


@component
def movie_card(movie, on_click):
    return div(class_name="movie-card", on_click=on_click,
        div(class_name="card-poster"),
        div(class_name="card-info",
            h3(movie["title"]),
            p(f"{movie['year']} • {movie['rating']}/10"),
        ),
    )


@component
def detail_panel(movie, on_close):
    return div(class_name="detail-overlay", on_click=on_close,
        div(class_name="detail-panel", on_click=lambda e: e.stopPropagation(),
            button(on_click=on_close, class_name="close-btn", "✕"),
            h2(movie["title"]),
            p(class_name="detail-meta", f"{movie['year']} • Rating: {movie['rating']}/10"),
            p(class_name="detail-description", movie["description"]),
            button(on_click=on_close, class_name="btn-secondary", "Close"),
        ),
    )
```
