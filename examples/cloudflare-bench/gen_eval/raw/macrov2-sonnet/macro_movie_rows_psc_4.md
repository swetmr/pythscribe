<!-- exp=macrov2-sonnet task=macro_movie_rows cond=psc sample=4 model=claude-sonnet-5 cost_usd=0.0323919 -->
```python
"use client"
R*

TRENDING = [
    {"id": 1, "title": "Neon Horizon", "year": 2024, "rating": 8.4,
     "description": "A hacker uncovers a conspiracy hidden inside a city-wide AI grid."},
    {"id": 2, "title": "The Last Ember", "year": 2022, "rating": 7.9,
     "description": "Survivors of a collapsed world search for the last living forest."},
    {"id": 3, "title": "Glass Tide", "year": 2023, "rating": 8.1,
     "description": "A marine biologist discovers a signal broadcasting from the deep ocean."},
    {"id": 4, "title": "Iron Choir", "year": 2021, "rating": 7.3,
     "description": "A disbanded band of soldiers reunites for one final mission."},
]

NEW = [
    {"id": 5, "title": "Paper Moonlight", "year": 2026, "rating": 8.7,
     "description": "Two estranged siblings retrace their late mother's final road trip."},
    {"id": 6, "title": "Static Bloom", "year": 2026, "rating": 7.6,
     "description": "A gardener in a dying colony ship grows the last seeds of Earth."},
    {"id": 7, "title": "Velvet Circuit", "year": 2025, "rating": 8.0,
     "description": "An underground racer is recruited into a corporate espionage ring."},
    {"id": 8, "title": "Hollow Signal", "year": 2025, "rating": 7.2,
     "description": "A radio operator picks up transmissions from a town that no longer exists."},
]

FEATURED = {"id": 0, "title": "Crimson Dawn", "year": 2026, "rating": 9.1,
    "description": "When the sun stops rising over one city, a small crew of engineers races to find out why before the world outside stops believing it ever will."}

def MovieCard(movie, on_select):
    return div(cn="movie-card", oc=lambda: on_select(movie), key=movie["id"],
        div(cn="movie-card-poster", movie["title"][0]),
        div(cn="movie-card-info",
            p(cn="movie-card-title", movie["title"]),
            p(cn="movie-card-meta", f"{movie['year']} · {movie['rating']:.1f}"),
        ),
    )
MovieCard = component(MovieCard)

def MovieRow(title, movies, on_select):
    return section(cn="movie-row",
        h2(cn="movie-row-title", title),
        div(cn="movie-row-list",
            *[MovieCard(m, on_select) for m in movies],
        ),
    )
MovieRow = component(MovieRow)

def DetailPanel(movie, on_close):
    return div(cn="detail-panel",
        button(cn="detail-close", oc=lambda: on_close(), "Close"),
        h2(cn="detail-title", movie["title"]),
        p(cn="detail-meta", f"{movie['year']} · Rating {movie['rating']:.1f}"),
        p(cn="detail-description", movie["description"]),
    )
DetailPanel = component(DetailPanel)

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
            p(cn="hero-meta", f"{FEATURED['year']} · Rating {FEATURED['rating']:.1f}"),
            p(cn="hero-description", FEATURED["description"]),
        ),
        DetailPanel(selected, close) if selected else None,
        MovieRow("Trending", TRENDING, select),
        MovieRow("New", NEW, select),
    )
```
