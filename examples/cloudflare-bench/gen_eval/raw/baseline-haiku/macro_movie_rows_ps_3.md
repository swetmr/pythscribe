<!-- exp=baseline-haiku task=macro_movie_rows cond=ps sample=3 model=claude-haiku-4-5-20251001 cost_usd=0.037874200000000004 -->
```python
"use client"

from pyths.react import component, use_state

@component
def MovieBrowser():
    selected_id, set_selected_id = use_state(None)
    
    featured = {
        "id": "featured",
        "title": "The Quantum Realm",
        "year": 2024,
        "rating": 8.5,
        "description": "A mind-bending sci-fi epic exploring the nature of reality itself."
    }
    
    trending = [
        {"id": "t1", "title": "Neon Dreams", "year": 2024, "rating": 8.2, "description": "A cyberpunk thriller set in a dystopian future."},
        {"id": "t2", "title": "Echoes of Time", "year": 2023, "rating": 7.9, "description": "An emotional journey through parallel timelines."},
        {"id": "t3", "title": "The Last Archive", "year": 2024, "rating": 8.4, "description": "A mystery unfolds in the world's last library."},
        {"id": "t4", "title": "Cosmic Wanderer", "year": 2023, "rating": 7.8, "description": "Follow an astronaut's quest across the galaxy."},
        {"id": "t5", "title": "Midnight Protocol", "year": 2024, "rating": 8.1, "description": "Hackers race against time in this tech thriller."},
    ]
    
    new = [
        {"id": "n1", "title": "Silent Woods", "year": 2024, "rating": 8.3, "description": "A psychological horror set in an eerie forest."},
        {"id": "n2", "title": "City Lights", "year": 2024, "rating": 7.7, "description": "A romantic drama spanning multiple generations."},
        {"id": "n3", "title": "Beyond Infinity", "year": 2024, "rating": 8.0, "description": "A coming-of-age space adventure for all ages."},
        {"id": "n4", "title": "The Forgotten Song", "year": 2024, "rating": 8.6, "description": "A musical biography of an iconic performer."},
        {"id": "n5", "title": "Digital Hearts", "year": 2024, "rating": 7.6, "description": "Two AIs discover what it means to fall in love."},
    ]
    
    selected_movie = None
    if selected_id == "featured":
        selected_movie = featured
    else:
        for m in trending + new:
            if m["id"] == selected_id:
                selected_movie = m
                break
    
    def on_card_click(movie_id):
        set_selected_id(movie_id)
    
    def on_close():
        set_selected_id(None)
    
    return div(class_name="movie-browser",
        div(class_name="hero",
            div(class_name="hero-content",
                h1(featured["title"]),
                p(f"Release: {featured['year']} | Rating: {featured['rating']}/10"),
                p(featured["description"]),
                button(on_click=lambda: on_card_click("featured"), class_name="hero-button", "Watch Now"),
            ),
        ),
        
        div(class_name="browse-section",
            h2("Trending Now"),
            div(class_name="movie-row",
                *[MovieCard(key=m["id"], movie=m, on_click=lambda mid=m["id"]: on_card_click(mid))
                  for m in trending],
            ),
        ),
        
        div(class_name="browse-section",
            h2("New Releases"),
            div(class_name="movie-row",
                *[MovieCard(key=m["id"], movie=m, on_click=lambda mid=m["id"]: on_card_click(mid))
                  for m in new],
            ),
        ),
        
        selected_movie and DetailPanel(movie=selected_movie, on_close=on_close),
    )

@component
def MovieCard(movie, on_click):
    return div(class_name="movie-card", on_click=on_click,
        div(class_name="card-poster", "📽️"),
        h3(movie["title"]),
        p(class_name="card-meta", f"{movie['year']} • {movie['rating']}/10"),
    )

@component
def DetailPanel(movie, on_close):
    def stop_propagation(e):
        e.stopPropagation()
    
    return div(class_name="detail-overlay", on_click=on_close,
        div(class_name="detail-panel", on_click=stop_propagation,
            div(class_name="detail-header",
                h2(movie["title"]),
                button(class_name="close-btn", on_click=on_close, "✕"),
            ),
            p(class_name="detail-meta", f"Release: {movie['year']} | Rating: {movie['rating']}/10"),
            p(class_name="detail-description", movie["description"]),
        ),
    )
```
