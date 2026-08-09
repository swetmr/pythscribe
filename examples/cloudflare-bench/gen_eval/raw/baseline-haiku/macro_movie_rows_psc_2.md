<!-- exp=baseline-haiku task=macro_movie_rows cond=psc sample=2 model=claude-haiku-4-5-20251001 cost_usd=0.0571984 -->
```python
"use client"
R*

MOVIES_TRENDING = [
    {"id": 1, "title": "Quantum Nexus", "year": 2024, "rating": 8.7, "description": "A mind-bending sci-fi thriller about parallel dimensions."},
    {"id": 2, "title": "Midnight Echo", "year": 2024, "rating": 8.4, "description": "A noir detective story set in a cyberpunk city."},
    {"id": 3, "title": "Steel Horizon", "year": 2024, "rating": 8.1, "description": "An epic space opera spanning galaxies."},
    {"id": 4, "title": "The Last Code", "year": 2024, "rating": 7.9, "description": "A hacker's race against an AI uprising."},
]

MOVIES_NEW = [
    {"id": 5, "title": "Echoes of Forever", "year": 2025, "rating": 8.2, "description": "A romantic drama across time and space."},
    {"id": 6, "title": "Shadow Protocol", "year": 2025, "rating": 8.5, "description": "A spy thriller with unexpected twists."},
    {"id": 7, "title": "Neon Dreams", "year": 2025, "rating": 7.8, "description": "An indie film about chasing impossible dreams."},
    {"id": 8, "title": "The Witness", "year": 2025, "rating": 8.3, "description": "A gripping mystery that challenges perception."},
]

FEATURED = {
    "title": "Quantum Nexus",
    "year": 2024,
    "rating": 8.7,
    "description": "A mind-bending sci-fi thriller about parallel dimensions colliding. Experience the ultimate reality-warping adventure.",
}

@c
def MovieCard(movie, on_select):
    return div(cn="movie-card", oc=lambda: on_select(movie),
        div(cn="card-poster", movie["title"]),
        div(cn="card-info",
            p(cn="card-title", movie["title"]),
            p(cn="card-meta", f"{movie['year']} • {movie['rating']}★"),
        ),
    )

@c
def MovieRow(title, movies, on_select):
    return div(cn="row",
        h3(cn="row-title", title),
        div(cn="row-cards",
            *[MovieCard(m, on_select) for m in movies],
        ),
    )

@c
def DetailPanel(movie, on_close):
    def handle_click(e):
        if e.target is e.currentTarget:
            on_close()
    
    return div(cn="panel-overlay", oc=handle_click,
        div(cn="panel-content",
            button(cn="panel-close", oc=lambda: on_close(), "Close"),
            h2(cn="panel-title", movie["title"]),
            p(cn="panel-meta", f"{movie['year']} • {movie['rating']}★"),
            p(cn="panel-desc", movie["description"]),
        ),
    )

@c
def MovieBrowser():
    selected, set_selected = us(None)
    
    return div(cn="browser",
        div(cn="hero",
            h1(cn="hero-title", FEATURED["title"]),
            p(cn="hero-meta", f"{FEATURED['year']} • {FEATURED['rating']}★"),
            p(cn="hero-desc", FEATURED["description"]),
        ),
        MovieRow("Trending", MOVIES_TRENDING, set_selected),
        MovieRow("New", MOVIES_NEW, set_selected),
        selected and DetailPanel(selected, lambda: set_selected(None)),
    )
```
