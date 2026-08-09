<!-- exp=baseline-sonnet task=macro_movie_rows cond=psc sample=4 model=claude-sonnet-5 cost_usd=0.030939599999999998 -->
```python
"use client"
R*

TRENDING = [
    {"id": 1, "title": "Neon Horizon", "year": 2024, "rating": 8.7, "description": "A rogue pilot races across a dying galaxy to deliver a cure before time runs out."},
    {"id": 2, "title": "Glass Tide", "year": 2022, "rating": 7.9, "description": "Two rival oceanographers uncover a city buried beneath the waves."},
    {"id": 3, "title": "Iron Season", "year": 2023, "rating": 8.2, "description": "A blacksmith's daughter leads a rebellion against an empire of machines."},
    {"id": 4, "title": "Paper Moons", "year": 2021, "rating": 7.4, "description": "An origami artist discovers her creations are windows into another world."},
]

NEW_RELEASES = [
    {"id": 5, "title": "Static Bloom", "year": 2026, "rating": 8.0, "description": "A radio signal from an abandoned station reawakens a forgotten war."},
    {"id": 6, "title": "Velvet Circuit", "year": 2026, "rating": 7.6, "description": "A jazz musician wired with experimental tech chases one final performance."},
    {"id": 7, "title": "Ash & Amber", "year": 2025, "rating": 8.5, "description": "Two siblings inherit a vineyard hiding centuries of buried secrets."},
    {"id": 8, "title": "Quiet Static", "year": 2025, "rating": 7.1, "description": "A sound engineer records silence and finds it isn't silent at all."},
]

FEATURED = {"id": 100, "title": "Crimson Ledger", "year": 2026, "rating": 9.1, "description": "A forensic accountant uncovers a conspiracy that reaches the highest levels of government, and must decide whether the truth is worth losing everything."}

def MovieCard(movie, on_select):
    return div(cn="movie-card", oc=lambda: on_select(movie), key=movie["id"],
        div(cn="movie-card-poster", movie["title"][0]),
        div(cn="movie-card-info",
            p(cn="movie-card-title", movie["title"]),
            p(cn="movie-card-meta", f"{movie['year']} · {movie['rating']:.1f}"),
        ),
    )

def MovieRow(title, movies, on_select):
    return div(cn="movie-row",
        h2(cn="movie-row-title", title),
        div(cn="movie-row-list", *[MovieCard(m, on_select) for m in movies]),
    )

def DetailPanel(movie, on_close):
    return div(cn="detail-overlay",
        div(cn="detail-panel",
            div(cn="detail-header",
                h2(movie["title"]),
                button(cn="detail-close", oc=lambda: on_close(), "Close"),
            ),
            p(cn="detail-meta", f"{movie['year']} · Rating {movie['rating']:.1f}"),
            p(cn="detail-description", movie["description"]),
        ),
    )

@c
def MovieBrowser():
    selected, set_selected = us(None)

    def select_movie(movie):
        set_selected(movie)

    def close_panel():
        set_selected(None)

    return div(cn="movie-browser",
        div(cn="hero",
            h1(cn="hero-title", FEATURED["title"]),
            p(cn="hero-meta", f"{FEATURED['year']} · Rating {FEATURED['rating']:.1f}"),
            p(cn="hero-description", FEATURED["description"]),
        ),
        MovieRow("Trending", TRENDING, select_movie),
        MovieRow("New", NEW_RELEASES, select_movie),
        DetailPanel(selected, close_panel) if selected else None,
    )
```
