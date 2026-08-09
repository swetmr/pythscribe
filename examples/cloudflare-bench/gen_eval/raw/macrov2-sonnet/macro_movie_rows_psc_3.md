<!-- exp=macrov2-sonnet task=macro_movie_rows cond=psc sample=3 model=claude-sonnet-5 cost_usd=0.0320469 -->
```python
"use client"
R*

MOVIES = {
    "featured": {"title": "Skyline Drift", "year": 2024, "rating": 8.7,
                 "description": "A getaway driver in a neon city discovers the heist he's driving for isn't what it seems."},
    "trending": [
        {"id": 1, "title": "Skyline Drift", "year": 2024, "rating": 8.7,
         "description": "A getaway driver in a neon city discovers the heist he's driving for isn't what it seems."},
        {"id": 2, "title": "Glass Orchard", "year": 2022, "rating": 7.9,
         "description": "Three estranged siblings return to their family orchard to settle a decades-old debt."},
        {"id": 3, "title": "Ashen Coast", "year": 2023, "rating": 8.1,
         "description": "A lighthouse keeper uncovers a smuggling ring hidden along the fog-bound coast."},
        {"id": 4, "title": "Paper Tigers", "year": 2021, "rating": 7.3,
         "description": "A washed-up boxer trains a rebellious teen for one last shot at the regional title."},
    ],
    "new": [
        {"id": 5, "title": "Nightshade Alley", "year": 2026, "rating": 6.8,
         "description": "A rookie detective chases a poisoner through the underbelly of a rain-soaked city."},
        {"id": 6, "title": "Hollow Meridian", "year": 2026, "rating": 7.5,
         "description": "A stranded astronaut must ration hope as much as oxygen on a dying space station."},
        {"id": 7, "title": "Velvet Static", "year": 2025, "rating": 6.9,
         "description": "A retired rock star's comeback tour is haunted by echoes of her old band."},
        {"id": 8, "title": "Marigold Pact", "year": 2025, "rating": 7.7,
         "description": "Two rival farming families are forced into an uneasy alliance to survive a drought."},
    ],
}


@c
def MovieCard(movie, on_select):
    return div(cn="movie-card", oc=lambda: on_select(movie),
        div(cn="movie-card-poster", movie["title"][0]),
        div(cn="movie-card-info",
            p(cn="movie-card-title", movie["title"]),
            p(cn="movie-card-meta", f"{movie['year']} · {movie['rating']}"),
        ),
    )


@c
def MovieRow(title, movies, on_select):
    return div(cn="movie-row",
        h2(cn="movie-row-title", title),
        div(cn="movie-row-track",
            *[MovieCard(movie=m, on_select=on_select) for m in movies],
        ),
    )


@c
def DetailPanel(movie, on_close):
    return div(cn="detail-panel",
        button(cn="detail-panel-close", oc=lambda: on_close(), "Close"),
        h2(cn="detail-panel-title", movie["title"]),
        p(cn="detail-panel-meta", f"{movie['year']} · {movie['rating']}"),
        p(cn="detail-panel-description", movie["description"]),
    )


@c
def MovieBrowser():
    selected, set_selected = us(None)

    def select(movie):
        set_selected(movie)

    def close():
        set_selected(None)

    featured = MOVIES["featured"]

    return div(cn="movie-browser",
        div(cn="hero",
            h1(cn="hero-title", featured["title"]),
            p(cn="hero-meta", f"{featured['year']} · {featured['rating']}"),
            p(cn="hero-description", featured["description"]),
        ),
        DetailPanel(movie=selected, on_close=close) if selected else None,
        MovieRow(title="Trending", movies=MOVIES["trending"], on_select=select),
        MovieRow(title="New", movies=MOVIES["new"], on_select=select),
    )
```
