<!-- exp=baseline-001 task=macro_movie_rows cond=psc sample=4 model=claude-opus-4-8 cost_usd=0.05001149999999999 -->
```python
"use client"
R*

trending = [
    {"title": "Neon Skyline", "year": 2023, "rating": 8.4, "description": "A courier races through a rain-soaked megacity to outrun the syndicate she once served."},
    {"title": "Deep Signal", "year": 2021, "rating": 7.9, "description": "A marine biologist decodes a message from the abyss that rewrites what we know of life."},
    {"title": "Paper Moons", "year": 2022, "rating": 8.1, "description": "Two grifters con their way across a shrinking planet, chasing one last honest score."},
    {"title": "Ashfall", "year": 2020, "rating": 7.5, "description": "When a dormant volcano wakes, a small town must decide who gets a seat on the last bus out."},
]

new_releases = [
    {"title": "Quiet Orbit", "year": 2024, "rating": 8.8, "description": "A lone astronaut befriends the station AI as her return window quietly closes."},
    {"title": "The Long Green", "year": 2024, "rating": 7.7, "description": "A retired ranger returns to the forest that took everything to plant one final grove."},
    {"title": "Midnight Cartography", "year": 2025, "rating": 8.6, "description": "A cartographer maps streets that only exist between midnight and dawn."},
    {"title": "Static Bloom", "year": 2025, "rating": 8.0, "description": "In a city where memories are broadcast, one woman guards the last private thought."},
]

featured = {
    "title": "Neon Skyline",
    "year": 2023,
    "rating": 8.4,
    "description": "A courier races through a rain-soaked megacity to outrun the syndicate she once served. Featured this week on the browse page.",
}


def stars(rating):
    return f"★ {rating:.1f}"


@c
def MovieCard(movie, on_open):
    return button(cn="movie-card", oc=lambda: on_open(movie),
        div(cn="movie-card-poster",
            span(cn="movie-card-initial", movie["title"][0]),
        ),
        div(cn="movie-card-body",
            span(cn="movie-card-title", movie["title"]),
            span(cn="movie-card-meta", f"{movie['year']} · {stars(movie['rating'])}"),
        ),
    )


@c
def MovieRow(title, movies, on_open):
    return section(cn="movie-row",
        h3(cn="movie-row-title", title),
        div(cn="movie-row-track",
            *[MovieCard(key=m["title"], movie=m, on_open=on_open) for m in movies],
        ),
    )


@c
def DetailPanel(movie, on_close):
    return div(cn="detail-panel",
        div(cn="detail-panel-inner",
            div(cn="detail-panel-header",
                h2(cn="detail-panel-title", movie["title"]),
                button(cn="detail-panel-close", oc=lambda: on_close(), "Close"),
            ),
            div(cn="detail-panel-meta",
                span(cn="detail-panel-year", str(movie["year"])),
                span(cn="detail-panel-rating", stars(movie["rating"])),
            ),
            p(cn="detail-panel-desc", movie["description"]),
        ),
    )


@c
def MovieBrowser():
    selected, set_selected = us(None)

    def open_movie(m):
        set_selected(m)

    def close():
        set_selected(None)

    return div(cn="movie-browser",
        section(cn="hero",
            div(cn="hero-content",
                span(cn="hero-badge", "Featured"),
                h1(cn="hero-title", featured["title"]),
                div(cn="hero-meta",
                    span(cn="hero-year", str(featured["year"])),
                    span(cn="hero-rating", stars(featured["rating"])),
                ),
                p(cn="hero-desc", featured["description"]),
                button(cn="hero-play", oc=lambda: open_movie(featured), "View details"),
            ),
        ),
        DetailPanel(movie=selected, on_close=close) if selected else None,
        div(cn="movie-rows",
            MovieRow(title="Trending", movies=trending, on_open=open_movie),
            MovieRow(title="New", movies=new_releases, on_open=open_movie),
        ),
    )
```
