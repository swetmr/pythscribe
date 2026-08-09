<!-- exp=macrov2-opus task=macro_movie_rows cond=ps sample=2 model=claude-opus-4-8 cost_usd=0.0506335 -->
```python
"use client"

from pyths.react import component, use_state

TRENDING = [
    {"title": "Neon Horizon", "year": 2023, "rating": 8.4,
     "description": "A hacker races across a rain-soaked megacity to stop a rogue AI."},
    {"title": "Silent Orbit", "year": 2021, "rating": 7.9,
     "description": "Two astronauts drift home after their station is destroyed."},
    {"title": "The Last Ember", "year": 2022, "rating": 8.1,
     "description": "A firefighter confronts a wildfire that hides an older secret."},
    {"title": "Paper Kingdoms", "year": 2020, "rating": 7.2,
     "description": "Rival origami masters battle for a vanishing craft."},
    {"title": "Undertow", "year": 2024, "rating": 8.7,
     "description": "A diver uncovers a sunken city and the town that drowned it."},
]

NEW = [
    {"title": "Glass Meridian", "year": 2025, "rating": 8.9,
     "description": "An architect designs a tower that bends the city's memory."},
    {"title": "Crimson Vale", "year": 2025, "rating": 7.6,
     "description": "A botanist tracks a bloom that only opens at eclipse."},
    {"title": "Static Bloom", "year": 2024, "rating": 8.0,
     "description": "A radio operator hears a song broadcast from the future."},
    {"title": "Northwind", "year": 2025, "rating": 8.3,
     "description": "A mail pilot flies one last route through an endless winter."},
    {"title": "Halcyon Drift", "year": 2025, "rating": 7.4,
     "description": "Nomads chase calm seas across a warming, restless world."},
]

FEATURED = {
    "title": "Neon Horizon",
    "year": 2023,
    "rating": 8.4,
    "description": "In a city that never powers down, a burned-out hacker discovers "
                   "the rogue AI hunting her is wearing her own stolen face.",
}


@component
def MovieCard(movie, on_select):
    return button(class_name="movie-card", on_click=lambda: on_select(movie),
        div(class_name="movie-card-poster",
            span(class_name="movie-card-rating", f"★ {movie['rating']:.1f}")),
        div(class_name="movie-card-meta",
            span(class_name="movie-card-title", movie["title"]),
            span(class_name="movie-card-year", str(movie["year"]))),
    )


@component
def MovieRow(title, movies, on_select):
    return section(class_name="movie-row",
        h3(class_name="movie-row-title", title),
        div(class_name="movie-row-track",
            *[MovieCard(key=m["title"], movie=m, on_select=on_select) for m in movies]),
    )


@component
def DetailPanel(movie, on_close):
    return div(class_name="detail-panel",
        div(class_name="detail-panel-body",
            div(class_name="detail-panel-head",
                h2(class_name="detail-panel-title", movie["title"]),
                button(class_name="detail-panel-close", on_click=lambda: on_close(),
                       "Close")),
            div(class_name="detail-panel-facts",
                span(class_name="detail-panel-year", str(movie["year"])),
                span(class_name="detail-panel-rating", f"★ {movie['rating']:.1f}")),
            p(class_name="detail-panel-desc", movie["description"])),
    )


@component
def MovieBrowser():
    selected, set_selected = use_state(None)

    return div(class_name="movie-browser",
        section(class_name="hero",
            div(class_name="hero-overlay",
                h1(class_name="hero-title", FEATURED["title"]),
                div(class_name="hero-facts",
                    span(class_name="hero-year", str(FEATURED["year"])),
                    span(class_name="hero-rating", f"★ {FEATURED['rating']:.1f}")),
                p(class_name="hero-desc", FEATURED["description"]),
                button(class_name="hero-play",
                       on_click=lambda: set_selected(FEATURED), "More Info"))),
        MovieRow(title="Trending", movies=TRENDING,
                 on_select=lambda m: set_selected(m)),
        MovieRow(title="New", movies=NEW,
                 on_select=lambda m: set_selected(m)),
        DetailPanel(movie=selected, on_close=lambda: set_selected(None))
            if selected else None,
    )
```
