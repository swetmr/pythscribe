<!-- exp=baseline-haiku task=macro_movie_rows cond=psc sample=1 model=claude-haiku-4-5-20251001 cost_usd=0.0425123 -->
```python
"use client"
R*

movies_data = [
    {"id": 1, "title": "Cosmic Journey", "year": 2024, "rating": 8.5, "category": "trending", "description": "An epic space exploration adventure across the galaxy."},
    {"id": 2, "title": "Midnight Echo", "year": 2024, "rating": 8.2, "category": "trending", "description": "A mysterious thriller set in a neon-lit cyberpunk city."},
    {"id": 3, "title": "The Last Stand", "year": 2023, "rating": 7.8, "category": "trending", "description": "A gripping survival story against impossible odds."},
    {"id": 4, "title": "Echoes of Home", "year": 2024, "rating": 8.8, "category": "new", "description": "A heartwarming drama about reconnecting with the past."},
    {"id": 5, "title": "Neon Dreams", "year": 2024, "rating": 8.3, "category": "new", "description": "A visually stunning sci-fi romance in a futuristic world."},
    {"id": 6, "title": "Silent Pursuit", "year": 2024, "rating": 8.1, "category": "new", "description": "An intense action thriller with incredible twists."},
]

featured_movie = {
    "title": "Cosmic Journey",
    "year": 2024,
    "rating": 8.5,
    "description": "An epic space exploration adventure across the galaxy. Experience breathtaking visuals and an unforgettable story.",
}

@c
def MovieCard(movie, on_select):
    return div(cn="movie-card", oc=on_select,
        div(cn="card-image-placeholder"),
        h3(cn="card-title", movie["title"]),
        p(cn="card-meta", f"{movie['year']} • ★ {movie['rating']}")
    )

@c
def DetailPanel(movie, on_close):
    def handle_panel_click(e):
        e.stopPropagation()
    
    return div(cn="detail-overlay", oc=on_close,
        div(cn="detail-panel", oc=handle_panel_click,
            button(cn="close-btn", oc=on_close, "✕"),
            h2(cn="detail-title", movie["title"]),
            p(cn="detail-meta", f"{movie['year']} • Rating: {movie['rating']}/10"),
            p(cn="detail-description", movie["description"])
        )
    )

@c
def MovieBrowser():
    selected, set_selected = us(None)
    
    trending = [m for m in movies_data if m["category"] == "trending"]
    new_movies = [m for m in movies_data if m["category"] == "new"]
    
    def select_movie(mid):
        set_selected(next((m for m in movies_data if m["id"] == mid), None))
    
    def close_detail():
        set_selected(None)
    
    return div(cn="movie-browser",
        div(cn="hero-section",
            h1("Featured: " + featured_movie["title"]),
            p(cn="hero-description", featured_movie["description"]),
            p(cn="hero-meta", f"{featured_movie['year']} • ★ {featured_movie['rating']}")
        ),
        div(cn="row-container",
            h2(cn="row-title", "Trending"),
            div(cn="movies-row",
                *[MovieCard(key=m["id"], movie=m, on_select=lambda mid=m["id"]: select_movie(mid)) for m in trending]
            )
        ),
        div(cn="row-container",
            h2(cn="row-title", "New Releases"),
            div(cn="movies-row",
                *[MovieCard(key=m["id"], movie=m, on_select=lambda mid=m["id"]: select_movie(mid)) for m in new_movies]
            )
        ),
        selected and DetailPanel(movie=selected, on_close=close_detail)
    )
```
