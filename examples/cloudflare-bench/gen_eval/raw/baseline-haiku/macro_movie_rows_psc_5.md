<!-- exp=baseline-haiku task=macro_movie_rows cond=psc sample=5 model=claude-haiku-4-5-20251001 cost_usd=0.056368400000000006 -->
```python
"use client"
R*

FEATURED = {
    "title": "The Quantum Realm",
    "year": 2024,
    "rating": 8.9,
    "description": "A mind-bending journey through dimensions of pure possibility.",
}

TRENDING = [
    {"id": 1, "title": "Stellar Voyage", "year": 2024, "rating": 8.5, "description": "Epic space exploration adventure."},
    {"id": 2, "title": "Digital Dreams", "year": 2023, "rating": 8.2, "description": "A hacker's tale in the neon city."},
    {"id": 3, "title": "Lost Tomorrow", "year": 2024, "rating": 7.9, "description": "Time travel gone wrong in the future."},
    {"id": 4, "title": "Echo Protocol", "year": 2023, "rating": 8.4, "description": "Artificial intelligence awakens."},
]

NEW = [
    {"id": 5, "title": "Neon Nights", "year": 2024, "rating": 7.6, "description": "A noir thriller in a cyberpunk metropolis."},
    {"id": 6, "title": "Whispers", "year": 2024, "rating": 8.1, "description": "A haunting supernatural mystery."},
    {"id": 7, "title": "Chromatic", "year": 2024, "rating": 7.8, "description": "A visual odyssey through color and sound."},
    {"id": 8, "title": "Rift", "year": 2024, "rating": 8.3, "description": "When reality begins to tear apart."},
]

def MovieCard(movie, on_click):
    return div(cn="card", oc=lambda: on_click(movie),
        div(cn="card-poster"),
        h3(movie["title"]),
        div(cn="card-meta",
            span(f"{movie['year']}"),
            span(f"★ {movie['rating']}"),
        ),
    )

def DetailPanel(movie, on_close):
    return div(cn="modal-backdrop", oc=lambda: on_close(),
        div(cn="modal", oc=lambda e: e.stopPropagation(),
            h2(movie["title"]),
            div(cn="detail-meta",
                span(f"Year: {movie['year']}"),
                span(f"Rating: {movie['rating']} / 10"),
            ),
            p(movie["description"]),
            button(oc=lambda: on_close(), cn="close-btn", "Close"),
        ),
    )

@c
def MovieBrowser():
    selected, set_selected = us(None)

    return div(cn="browser",
        div(cn="hero",
            div(cn="hero-content",
                h1(FEATURED["title"]),
                p(f"⭐ {FEATURED['rating']} • {FEATURED['year']}"),
                p(cn="hero-desc", FEATURED["description"]),
            ),
        ),
        div(cn="section",
            h2("Trending Now"),
            div(cn="row",
                *[MovieCard(m, set_selected) for m in TRENDING],
            ),
        ),
        div(cn="section",
            h2("New Releases"),
            div(cn="row",
                *[MovieCard(m, set_selected) for m in NEW],
            ),
        ),
        DetailPanel(selected, lambda: set_selected(None)) if selected else None,
    )
```
