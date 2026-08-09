<!-- exp=baseline-sonnet task=macro_movie_rows cond=psc sample=3 model=claude-sonnet-5 cost_usd=0.0346896 -->
```python
"use client"
R*

MOVIES = {
    "trending": [
        {"id": 1, "title": "Neon Drift", "year": 2024, "rating": 8.4,
         "description": "A getaway driver in a synthwave city discovers the heist he's running is a trap set by his own crew."},
        {"id": 2, "title": "Glass Horizon", "year": 2023, "rating": 7.8,
         "description": "Survivors of a collapsed space station drift toward a distant colony with dwindling oxygen."},
        {"id": 3, "title": "The Last Ember", "year": 2022, "rating": 8.1,
         "description": "A retired firefighter is pulled back into service when wildfires threaten his estranged daughter's town."},
        {"id": 4, "title": "Paper Moons", "year": 2024, "rating": 7.2,
         "description": "Two con artists fall for each other while running the same long game on opposite ends."},
        {"id": 5, "title": "Static Kingdom", "year": 2021, "rating": 8.6,
         "description": "In a world where memories can be broadcast, a hacker uncovers a conspiracy buried in her own past."},
    ],
    "new": [
        {"id": 6, "title": "Salt & Signal", "year": 2026, "rating": 7.5,
         "description": "A lighthouse keeper intercepts a distress call that shouldn't exist, decades after the ship went down."},
        {"id": 7, "title": "Midnight Ledger", "year": 2026, "rating": 8.0,
         "description": "An forensic accountant unravels a shell company that leads straight to her own firm's founders."},
        {"id": 8, "title": "Hollow Orbit", "year": 2025, "rating": 7.9,
         "description": "A maintenance crew aboard a dying satellite must choose who gets the last escape pod."},
        {"id": 9, "title": "Ashen Roads", "year": 2025, "rating": 8.3,
         "description": "A courier crossing a quarantined desert highway carries cargo everyone wants and no one can name."},
        {"id": 10, "title": "Velvet Static", "year": 2026, "rating": 7.1,
         "description": "A washed-up radio DJ finds his late-night show is somehow reaching listeners in the past."},
    ],
}

FEATURED = {
    "id": 0, "title": "Neon Drift", "year": 2024, "rating": 8.4,
    "description": "A getaway driver in a synthwave city discovers the heist he's running is a trap set by his own crew. Now he has one night to turn the tables before the crew — and the city — close in.",
}

def movie_card(movie, on_select):
    return div(cn="movie-card", key=movie["id"], oc=lambda: on_select(movie),
        div(cn="movie-card-poster", movie["title"][0]),
        div(cn="movie-card-info",
            p(cn="movie-card-title", movie["title"]),
            p(cn="movie-card-meta", f"{movie['year']} · {movie['rating']:.1f}"),
        ),
    )

def movie_row(title, movies, on_select):
    return div(cn="movie-row",
        h3(cn="movie-row-title", title),
        div(cn="movie-row-track", *[movie_card(m, on_select) for m in movies]),
    )

def detail_panel(movie, on_close):
    return div(cn="detail-panel",
        div(cn="detail-panel-header",
            h2(movie["title"]),
            button(cn="detail-close", oc=on_close, "Close"),
        ),
        p(cn="detail-meta", f"{movie['year']} · {movie['rating']:.1f} rating"),
        p(cn="detail-description", movie["description"]),
    )

@c
def MovieBrowser():
    selected, set_selected = us(None)

    def select(movie):
        set_selected(movie)

    def close():
        set_selected(None)

    return div(cn="movie-browser",
        div(cn="hero",
            h1(cn="hero-title", FEATURED["title"]),
            p(cn="hero-meta", f"{FEATURED['year']} · {FEATURED['rating']:.1f}"),
            p(cn="hero-description", FEATURED["description"]),
        ),
        detail_panel(selected, close) if selected else None,
        movie_row("Trending", MOVIES["trending"], select),
        movie_row("New", MOVIES["new"], select),
    )
```
