<!-- exp=baseline-sonnet task=macro_movie_rows cond=psc sample=2 model=claude-sonnet-5 cost_usd=0.0512196 -->
```python
"use client"
R*

FEATURED = {
    "title": "Nebula Drift",
    "description": "A stranded crew races against a collapsing star to find a way home before the light goes out forever.",
    "year": 2025,
    "rating": 8.7,
}

TRENDING = [
    {"title": "Nebula Drift", "year": 2025, "rating": 8.7, "description": "A stranded crew races against a collapsing star to find a way home before the light goes out forever."},
    {"title": "Iron Tide", "year": 2023, "rating": 7.9, "description": "A dockworker uncovers a smuggling ring that reaches the top of the city government."},
    {"title": "Paper Moons", "year": 2024, "rating": 8.1, "description": "Two estranged sisters reunite to save their late father's failing letterpress shop."},
    {"title": "Glass Horizon", "year": 2022, "rating": 7.4, "description": "An architect's dream city becomes a nightmare when its residents start disappearing."},
    {"title": "Static Bloom", "year": 2025, "rating": 8.3, "description": "A radio host's late-night broadcasts start predicting events before they happen."},
]

NEW_MOVIES = [
    {"title": "Ash & Ember", "year": 2026, "rating": 7.6, "description": "A retired firefighter is pulled back into service when wildfires threaten her hometown."},
    {"title": "Quiet Static", "year": 2026, "rating": 8.0, "description": "A sound engineer discovers a hidden message buried in decades of archived recordings."},
    {"title": "The Long Thaw", "year": 2026, "rating": 7.2, "description": "A remote research station must survive as the ice around them starts to melt too fast."},
    {"title": "Velvet Circuit", "year": 2026, "rating": 8.5, "description": "An underground street-racing crew builds an AI driver to outsmart a corrupt league."},
    {"title": "Harbor Lights", "year": 2026, "rating": 7.8, "description": "A lighthouse keeper's quiet life is upended by a shipwrecked stranger with a secret."},
]

def render_row(label, movies, on_select):
    return div(cn="row",
        h3(label),
        div(cn="row-cards",
            *[div(cn="card", key=m["title"], oc=lambda m=m: on_select(m),
                div(cn="card-title", m["title"]),
                div(cn="card-meta", f"{m['year']} • ⭐ {m['rating']}"),
            ) for m in movies]
        ),
    )

@c
def MovieBrowser():
    selected, set_selected = us(None)

    return div(cn="movie-browser",
        div(cn="hero",
            h1(cn="hero-title", FEATURED["title"]),
            p(cn="hero-desc", FEATURED["description"]),
            div(cn="hero-meta", f"{FEATURED['year']} • ⭐ {FEATURED['rating']}"),
        ),
        render_row("Trending", TRENDING, set_selected),
        render_row("New", NEW_MOVIES, set_selected),
        div(cn="detail-panel",
            h2(selected["title"]),
            div(cn="detail-meta", f"{selected['year']} • ⭐ {selected['rating']}"),
            p(selected["description"]),
            button(oc=lambda: set_selected(None), "Close"),
        ) if selected else None,
    )
```
