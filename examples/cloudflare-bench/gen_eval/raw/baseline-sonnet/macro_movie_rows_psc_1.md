<!-- exp=baseline-sonnet task=macro_movie_rows cond=psc sample=1 model=claude-sonnet-5 cost_usd=0.06084 -->
```python
"use client"
R*

MOVIES = [
    {"id": 1, "title": "Neon Horizon", "year": 2024, "rating": 8.4, "category": "Trending",
     "description": "A rogue pilot races across a dying galaxy to deliver a cure before the last star collapses."},
    {"id": 2, "title": "Glass Kingdom", "year": 2023, "rating": 7.9, "category": "Trending",
     "description": "Two rival dynasties battle for control of a floating city built entirely from glass."},
    {"id": 3, "title": "Midnight Ledger", "year": 2022, "rating": 8.1, "category": "Trending",
     "description": "An accountant uncovers a conspiracy that could unravel the world's financial system."},
    {"id": 4, "title": "Echo Valley", "year": 2025, "rating": 7.6, "category": "Trending",
     "description": "A small town wakes up to find every sound now repeats itself exactly one hour later."},
    {"id": 5, "title": "Paper Tigers", "year": 2026, "rating": 8.7, "category": "New",
     "description": "Retired martial artists reunite for one last job when their old master is threatened."},
    {"id": 6, "title": "Static Bloom", "year": 2026, "rating": 7.3, "category": "New",
     "description": "A botanist discovers a flower that grows only in the presence of electromagnetic interference."},
    {"id": 7, "title": "Hollow Signal", "year": 2026, "rating": 8.0, "category": "New",
     "description": "Deep-space researchers intercept a transmission that predicts their own deaths."},
    {"id": 8, "title": "Velvet Circuit", "year": 2025, "rating": 7.8, "category": "New",
     "description": "An underground street-racing crew builds a car that thinks for itself."},
]

FEATURED = MOVIES[0]

def MovieCard(movie, on_select):
    return div(cn="movie-card", oc=lambda: on_select(movie), key=movie["id"],
        div(cn="movie-card-thumb", movie["title"][0]),
        div(cn="movie-card-info",
            p(cn="movie-card-title", movie["title"]),
            p(cn="movie-card-meta", f"{movie['year']} · {movie['rating']}★"),
        ),
    )

def MovieRow(title, movies, on_select):
    return div(cn="movie-row",
        h2(cn="movie-row-title", title),
        div(cn="movie-row-track",
            *[MovieCard(m, on_select) for m in movies],
        ),
    )

def DetailPanel(movie, on_close):
    return div(cn="detail-panel",
        button(cn="detail-close", oc=on_close, "Close"),
        h2(cn="detail-title", movie["title"]),
        p(cn="detail-meta", f"{movie['year']} · {movie['rating']}★"),
        p(cn="detail-description", movie["description"]),
    )

@c
def MovieBrowser():
    selected, set_selected = us(None)

    trending = [m for m in MOVIES if m["category"] == "Trending"]
    new_releases = [m for m in MOVIES if m["category"] == "New"]

    def select_movie(movie):
        set_selected(movie)

    def close_detail():
        set_selected(None)

    return div(cn="movie-browser",
        div(cn="hero",
            h1(cn="hero-title", FEATURED["title"]),
            p(cn="hero-meta", f"{FEATURED['year']} · {FEATURED['rating']}★"),
            p(cn="hero-description", FEATURED["description"]),
        ),
        DetailPanel(selected, close_detail) if selected else None,
        MovieRow("Trending", trending, select_movie),
        MovieRow("New", new_releases, select_movie),
    )
```
