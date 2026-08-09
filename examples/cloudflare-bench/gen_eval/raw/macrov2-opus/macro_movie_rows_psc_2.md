<!-- exp=macrov2-opus task=macro_movie_rows cond=psc sample=2 model=claude-opus-4-8 cost_usd=0.0429945 -->
```python
"use client"
R*

TRENDING = [
    {"title": "Neon Drift", "year": 2021, "rating": 8.2, "description": "A street racer chases redemption through rain-soaked megacity nights."},
    {"title": "Iron Tide", "year": 2019, "rating": 7.6, "description": "A naval crew fights to survive a storm that should not exist."},
    {"title": "Paper Kingdoms", "year": 2022, "rating": 8.9, "description": "Rival origami masters wage a quiet war for a crumbling city."},
    {"title": "Glass Hour", "year": 2020, "rating": 7.1, "description": "Every choice loops back in a town where time is running out."},
]

NEW = [
    {"title": "Salt & Ember", "year": 2024, "rating": 8.5, "description": "Two chefs rebuild a burned-down bistro and an old friendship."},
    {"title": "Vanta", "year": 2024, "rating": 7.9, "description": "A pilot lost in deep space follows a signal only she can hear."},
    {"title": "The Long Quiet", "year": 2023, "rating": 8.0, "description": "After the noise ends, a family learns to speak again."},
    {"title": "Copperline", "year": 2024, "rating": 7.4, "description": "A wiretapper hears a murder before it happens."},
]

FEATURED = {
    "title": "Paper Kingdoms",
    "year": 2022,
    "rating": 8.9,
    "description": "Rival origami masters wage a quiet war for a crumbling city, folding paper into promises and betrayals.",
}

@c
def MovieCard(movie, on_open):
    return div(cn="movie-card", oc=lambda: on_open(movie),
        div(cn="movie-card-poster", movie["title"][0]),
        div(cn="movie-card-title", movie["title"]),
        div(cn="movie-card-meta", f"{movie['year']} · ★ {movie['rating']:.1f}"),
    )

@c
def MovieRow(title, movies, on_open):
    return section(cn="movie-row",
        h3(cn="movie-row-title", title),
        div(cn="movie-row-track",
            *[MovieCard(key=m["title"], movie=m, on_open=on_open) for m in movies]),
    )

@c
def MovieBrowser():
    selected, set_selected = us(None)

    return div(cn="movie-browser",
        section(cn="hero",
            div(cn="hero-body",
                h1(cn="hero-title", FEATURED["title"]),
                p(cn="hero-meta", f"{FEATURED['year']} · ★ {FEATURED['rating']:.1f}"),
                p(cn="hero-desc", FEATURED["description"]),
                button(cn="hero-play", oc=lambda: set_selected(FEATURED), "More Info"),
            ),
        ),
        MovieRow(title="Trending", movies=TRENDING, on_open=lambda m: set_selected(m)),
        MovieRow(title="New", movies=NEW, on_open=lambda m: set_selected(m)),
        div(cn="detail-panel",
            div(cn="detail-card",
                h2(cn="detail-title", selected["title"]),
                p(cn="detail-meta", f"{selected['year']} · ★ {selected['rating']:.1f}"),
                p(cn="detail-desc", selected["description"]),
                button(cn="detail-close", oc=lambda: set_selected(None), "Close"),
            ),
        ) if selected else None,
    )
```
