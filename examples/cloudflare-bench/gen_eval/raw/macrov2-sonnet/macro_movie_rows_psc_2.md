<!-- exp=macrov2-sonnet task=macro_movie_rows cond=psc sample=2 model=claude-sonnet-5 cost_usd=0.1125669 -->
```python
"use client"
R*

FEATURED = {
    "title": "The Crimson Horizon",
    "year": 2024,
    "rating": "8.7",
    "description": "A stranded astronaut uncovers a conspiracy that threatens to unravel humanity's return to Earth.",
}

TRENDING = [
    {"title": "Neon Static", "year": 2023, "rating": "7.9", "description": "A hacker collective races to expose a rogue AI before it seizes control of the city's grid."},
    {"title": "Glass Kingdom", "year": 2022, "rating": "8.1", "description": "Two rival families battle for control of a crumbling empire built on secrets and glass."},
    {"title": "Midnight Runners", "year": 2024, "rating": "7.4", "description": "A crew of street racers gets pulled into an international smuggling operation."},
    {"title": "Echoes of Us", "year": 2021, "rating": "8.5", "description": "A grieving scientist builds an AI companion modeled after her late partner."},
]

NEW = [
    {"title": "Salt & Ash", "year": 2026, "rating": "7.2", "description": "A fisherman's daughter uncovers an ancient curse buried beneath her coastal town."},
    {"title": "Paper Moons", "year": 2026, "rating": "8.0", "description": "An origami artist's creations begin appearing in real life, one folded secret at a time."},
    {"title": "The Last Signal", "year": 2025, "rating": "7.6", "description": "A radio operator on an isolated station picks up a transmission that shouldn't exist."},
    {"title": "Static Bloom", "year": 2026, "rating": "8.3", "description": "A botanist discovers a flower that grows only near electromagnetic disturbances."},
]

@c
def movie_card(movie, on_click):
    return div(cn="movie-card", key=movie["title"], oc=lambda: on_click(movie),
        div(cn="movie-poster", movie["title"][0]),
        h4(movie["title"]),
        p(cn="movie-meta", f'{movie["year"]} · {movie["rating"]}'),
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
            h1(FEATURED["title"]),
            p(cn="hero-meta", f'{FEATURED["year"]} · {FEATURED["rating"]}'),
            p(cn="hero-description", FEATURED["description"]),
        ),
        div(cn="detail-panel",
            button(cn="close-button", oc=lambda: close(), "Close"),
            h2(selected["title"]),
            p(cn="detail-meta", f'{selected["year"]} · {selected["rating"]}'),
            p(cn="detail-description", selected["description"]),
        ) if selected else None,
        div(cn="row",
            h3("Trending"),
            div(cn="row-items",
                *[movie_card(movie=m, on_click=select) for m in TRENDING],
            ),
        ),
        div(cn="row",
            h3("New"),
            div(cn="row-items",
                *[movie_card(movie=m, on_click=select) for m in NEW],
            ),
        ),
    )
```
