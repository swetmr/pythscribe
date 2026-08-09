<!-- exp=baseline-haiku task=macro_movie_rows cond=psc sample=4 model=claude-haiku-4-5-20251001 cost_usd=0.039383400000000006 -->
```python
"use client"
R*

@c
def MovieBrowser():
    selected_movie, set_selected_movie = us(None)
    
    featured = {
        "title": "Stellar Horizon",
        "year": 2024,
        "rating": "9.2",
        "description": "A groundbreaking sci-fi epic exploring the mysteries of deep space.",
        "hero_desc": "Journey beyond the stars in this visually stunning adventure."
    }
    
    trending = [
        {"id": 1, "title": "Neon Dreams", "year": 2024, "rating": "8.7", "description": "A neo-noir thriller set in a dystopian megacity."},
        {"id": 2, "title": "Silent Echo", "year": 2023, "rating": "8.3", "description": "A haunting mystery unfolds in an isolated lighthouse."},
        {"id": 3, "title": "Pulse", "year": 2024, "rating": "8.9", "description": "A high-tech heist against impossible odds."},
        {"id": 4, "title": "The Last Garden", "year": 2023, "rating": "8.5", "description": "A poignant drama about family and legacy."},
    ]
    
    new = [
        {"id": 5, "title": "Void", "year": 2024, "rating": "7.8", "description": "An experimental art film pushing boundaries."},
        {"id": 6, "title": "Crimson Tides", "year": 2024, "rating": "8.1", "description": "An epic historical drama spanning generations."},
        {"id": 7, "title": "Code Zero", "year": 2024, "rating": "8.4", "description": "A cyber-thriller in a near-future world."},
        {"id": 8, "title": "Whispers in the Dark", "year": 2024, "rating": "7.9", "description": "A supernatural horror tale with a twist."},
    ]
    
    return div(cn="browser",
        div(cn="hero",
            h1(featured["title"]),
            p(cn="hero-desc", featured["hero_desc"]),
            p(f"{featured['year']} • Rating: {featured['rating']}"),
        ),
        
        div(cn="section",
            h2("Trending"),
            div(cn="row",
                *[div(cn="card", oc=lambda m=movie: set_selected_movie(m),
                    h3(movie["title"]),
                    p(f"{movie['year']} • {movie['rating']}"),
                ) for movie in trending]
            ),
        ),
        
        div(cn="section",
            h2("New"),
            div(cn="row",
                *[div(cn="card", oc=lambda m=movie: set_selected_movie(m),
                    h3(movie["title"]),
                    p(f"{movie['year']} • {movie['rating']}"),
                ) for movie in new]
            ),
        ),
        
        selected_movie and div(cn="overlay", oc=lambda: set_selected_movie(None),
            div(cn="panel", oc=lambda e: e.stopPropagation(),
                button(cn="close", oc=lambda: set_selected_movie(None), "✕"),
                h2(selected_movie["title"]),
                p(f"{selected_movie['year']} • {selected_movie['rating']}"),
                p(selected_movie["description"]),
            ),
        ) or None,
    )
```
