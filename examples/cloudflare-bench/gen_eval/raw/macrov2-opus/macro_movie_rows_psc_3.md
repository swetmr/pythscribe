<!-- exp=macrov2-opus task=macro_movie_rows cond=psc sample=3 model=claude-opus-4-8 cost_usd=0.0420445 -->
```python
"use client"
R*

TRENDING = [
    {"title": "Neon Horizon", "year": 2023, "rating": "TV-MA",
     "desc": "A hacker chases a rogue AI across a rain-soaked megacity."},
    {"title": "Iron Tide", "year": 2021, "rating": "PG-13",
     "desc": "A stranded crew fights to keep their submarine from the deep."},
    {"title": "Paper Kingdoms", "year": 2022, "rating": "TV-14",
     "desc": "Rival origami masters wage a quiet war of folded empires."},
    {"title": "Last Ember", "year": 2024, "rating": "R",
     "desc": "The final firekeeper guards a dying flame against the frost."},
]

NEW = [
    {"title": "Glass Meridian", "year": 2025, "rating": "TV-MA",
     "desc": "Two architects design a tower that bends the coastline's light."},
    {"title": "Salt & Signal", "year": 2025, "rating": "PG",
     "desc": "A lighthouse keeper decodes messages hidden in the tides."},
    {"title": "Vermilion Run", "year": 2024, "rating": "TV-14",
     "desc": "A courier races a sealed package through a city that forgets."},
    {"title": "Hollow Choir", "year": 2025, "rating": "R",
     "desc": "A choir's lost voice returns with a song no one taught it."},
]

FEATURED = {
    "title": "Neon Horizon",
    "year": 2023,
    "rating": "TV-MA",
    "desc": "In a city that never sleeps, one hacker must outrun the AI she "
            "set loose before it rewrites the world in its own image.",
}


@c
def MovieCard(movie, on_open):
    return div(cn="movie-card", oc=lambda: on_open(movie),
        div(cn="movie-card-thumb"),
        div(cn="movie-card-meta",
            span(cn="movie-card-title", movie["title"]),
            span(cn="movie-card-year", f"{movie['year']} · {movie['rating']}"),
        ),
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
            div(cn="hero-backdrop"),
            div(cn="hero-body",
                h1(cn="hero-title", FEATURED["title"]),
                p(cn="hero-tags", f"{FEATURED['year']} · {FEATURED['rating']}"),
                p(cn="hero-desc", FEATURED["desc"]),
            ),
        ),
        MovieRow(title="Trending", movies=TRENDING, on_open=lambda m: set_selected(m)),
        MovieRow(title="New", movies=NEW, on_open=lambda m: set_selected(m)),
        div(cn="detail-panel",
            h2(cn="detail-title", selected["title"]),
            p(cn="detail-tags", f"{selected['year']} · {selected['rating']}"),
            p(cn="detail-desc", selected["desc"]),
            button(cn="detail-close", oc=lambda: set_selected(None), "Close"),
        ) if selected else None,
    )
```
