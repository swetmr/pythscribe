<!-- exp=macrov2-opus task=macro_movie_rows cond=psc sample=4 model=claude-opus-4-8 cost_usd=0.0478445 -->
```python
"use client"
R*

MOVIES = {
    "trending": [
        {"title": "Neon Skyline", "year": 2021, "rating": 8.4,
         "desc": "A rogue architect races to save a floating city from collapse."},
        {"title": "Silent Harbor", "year": 2019, "rating": 7.9,
         "desc": "Two strangers uncover a smuggling ring in a quiet fishing town."},
        {"title": "Crimson Orbit", "year": 2022, "rating": 8.8,
         "desc": "A stranded crew must repair their ship before the sun sets forever."},
        {"title": "Paper Kingdoms", "year": 2020, "rating": 7.2,
         "desc": "A cartographer maps lands that vanish the moment they are drawn."},
    ],
    "new": [
        {"title": "Glass Meridian", "year": 2024, "rating": 9.1,
         "desc": "A translator decodes a signal that rewrites human memory."},
        {"title": "Ashfall County", "year": 2023, "rating": 6.8,
         "desc": "A sheriff confronts a wildfire and the secrets it exposes."},
        {"title": "Velvet Circuit", "year": 2024, "rating": 8.0,
         "desc": "An underground pianist gets tangled in a heist gone electric."},
        {"title": "The Long Thaw", "year": 2023, "rating": 7.5,
         "desc": "A climatologist and her estranged son wait out an endless winter."},
    ],
}

FEATURED = {
    "title": "Nightfall Protocol",
    "year": 2025,
    "rating": 9.3,
    "desc": "When a citywide blackout hides a coordinated heist, a disgraced "
            "detective has one night to trace the signal before it goes dark for good.",
}


@c
def MovieCard(movie, on_open):
    return button(cn="movie-card", oc=on_open,
        div(cn="movie-card-poster", movie["title"][0]),
        div(cn="movie-card-body",
            span(cn="movie-card-title", movie["title"]),
            span(cn="movie-card-meta", f"{movie['year']} · ★ {movie['rating']:.1f}"),
        ),
    )


@c
def MovieRow(label, movies, on_select):
    return section(cn="movie-row",
        h3(cn="movie-row-label", label),
        div(cn="movie-row-track",
            *[MovieCard(key=m["title"], movie=m,
                       on_open=(lambda m=m: on_select(m)))
              for m in movies]),
    )


@c
def DetailPanel(movie, on_close):
    return div(cn="detail-panel",
        div(cn="detail-panel-inner",
            div(cn="detail-panel-head",
                h2(cn="detail-panel-title", movie["title"]),
                button(cn="detail-panel-close", oc=on_close, "Close"),
            ),
            p(cn="detail-panel-meta", f"{movie['year']} · ★ {movie['rating']:.1f}"),
            p(cn="detail-panel-desc", movie["desc"]),
        ),
    )


@c
def MovieBrowser():
    selected, set_selected = us(None)

    def select(movie):
        set_selected(movie)

    def close():
        set_selected(None)

    return div(cn="movie-browser",
        section(cn="hero",
            div(cn="hero-content",
                span(cn="hero-badge", "Featured"),
                h1(cn="hero-title", FEATURED["title"]),
                p(cn="hero-meta", f"{FEATURED['year']} · ★ {FEATURED['rating']:.1f}"),
                p(cn="hero-desc", FEATURED["desc"]),
                button(cn="hero-play", oc=lambda: select(FEATURED), "Play"),
            ),
        ),
        DetailPanel(movie=selected, on_close=lambda: close()) if selected else None,
        div(cn="movie-rows",
            MovieRow(label="Trending", movies=MOVIES["trending"], on_select=select),
            MovieRow(label="New", movies=MOVIES["new"], on_select=select),
        ),
    )
```
