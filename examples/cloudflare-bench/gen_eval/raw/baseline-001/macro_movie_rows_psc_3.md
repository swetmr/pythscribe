<!-- exp=baseline-001 task=macro_movie_rows cond=psc sample=3 model=claude-opus-4-8 cost_usd=0.041861499999999996 -->
```python
"use client"
R*

TRENDING = [
    {"title": "Neon Horizon", "year": 2023, "rating": 8.4, "description": "A rogue pilot races across a fractured skyline to outrun her past."},
    {"title": "Silent Harbor", "year": 2021, "rating": 7.9, "description": "Two strangers uncover a decades-old secret buried beneath a fishing town."},
    {"title": "Iron Bloom", "year": 2022, "rating": 8.1, "description": "In a war-torn valley, a botanist cultivates hope one seed at a time."},
    {"title": "Glass Kingdom", "year": 2024, "rating": 7.6, "description": "An heir must choose between a crumbling empire and a forbidden love."},
]

NEW = [
    {"title": "Midnight Cartography", "year": 2025, "rating": 8.8, "description": "A cartographer maps a city that only exists after dark."},
    {"title": "Paper Suns", "year": 2025, "rating": 7.3, "description": "A grieving artist folds origami that begins to alter reality."},
    {"title": "The Long Static", "year": 2024, "rating": 8.0, "description": "A radio operator on a remote station hears a voice that shouldn't exist."},
    {"title": "Copper Rain", "year": 2025, "rating": 7.7, "description": "A drifter returns home as strange metallic storms sweep the plains."},
]

FEATURED = {
    "title": "Neon Horizon",
    "description": "A rogue pilot races across a fractured skyline to outrun her past — and the syndicate that built her.",
}


def MovieCard(movie, on_open):
    return div(cn="movie-card", oc=lambda: on_open(movie),
        div(cn="movie-card-poster",
            span(cn="movie-card-rating", f"★ {movie['rating']:.1f}"),
        ),
        div(cn="movie-card-body",
            h3(cn="movie-card-title", movie["title"]),
            span(cn="movie-card-year", str(movie["year"])),
        ),
    )


def MovieRow(label, movies, on_open):
    return section(cn="movie-row",
        h2(cn="movie-row-title", label),
        div(cn="movie-row-track",
            *[MovieCard(m, on_open) for m in movies],
        ),
    )


@c
def MovieBrowser():
    selected, set_selected = us(None)

    return div(cn="movie-browser",
        section(cn="hero",
            div(cn="hero-content",
                span(cn="hero-badge", "Featured"),
                h1(cn="hero-title", FEATURED["title"]),
                p(cn="hero-description", FEATURED["description"]),
            ),
        ),
        div(cn="movie-rows",
            MovieRow("Trending", TRENDING, lambda m: set_selected(m)),
            MovieRow("New", NEW, lambda m: set_selected(m)),
        ),
        div(cn="detail-panel",
            div(cn="detail-panel-inner",
                h2(cn="detail-title", selected["title"]),
                div(cn="detail-meta",
                    span(cn="detail-year", str(selected["year"])),
                    span(cn="detail-rating", f"★ {selected['rating']:.1f}"),
                ),
                p(cn="detail-description", selected["description"]),
                button(cn="detail-close", oc=lambda: set_selected(None), "Close"),
            ),
        ) if selected else None,
    )
```
