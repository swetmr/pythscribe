<!-- exp=baseline-001 task=macro_movie_rows cond=ps sample=3 model=claude-opus-4-8 cost_usd=0.0552215 -->
```python
"use client"

from pyths.react import component, use_state

FEATURED = {
    "title": "The Silent Expanse",
    "year": 2023,
    "rating": 8.7,
    "description": "A lone navigator drifts through a dying galaxy, chasing a signal that may be the last echo of humanity.",
}

TRENDING = [
    {"id": "t1", "title": "The Silent Expanse", "year": 2023, "rating": 8.7,
     "description": "A lone navigator drifts through a dying galaxy, chasing a signal that may be the last echo of humanity."},
    {"id": "t2", "title": "Midnight Cartography", "year": 2021, "rating": 7.9,
     "description": "A grieving mapmaker discovers streets that only appear after dark."},
    {"id": "t3", "title": "Glasswing", "year": 2022, "rating": 8.1,
     "description": "Two rival entomologists race to protect a butterfly that shouldn't exist."},
    {"id": "t4", "title": "Ironbound", "year": 2020, "rating": 7.4,
     "description": "A blacksmith's daughter forges a rebellion against an unyielding empire."},
    {"id": "t5", "title": "Paper Tigers", "year": 2024, "rating": 8.3,
     "description": "An origami artist folds her way into a hidden dimension of memory."},
]

NEW = [
    {"id": "n1", "title": "Harbor Lights", "year": 2025, "rating": 7.6,
     "description": "A retired lighthouse keeper takes on one final, impossible storm."},
    {"id": "n2", "title": "The Ledger", "year": 2025, "rating": 8.0,
     "description": "An accountant uncovers a conspiracy hidden in decades of receipts."},
    {"id": "n3", "title": "Verdant", "year": 2024, "rating": 7.2,
     "description": "A botanist wakes plants that remember every hand that touched them."},
    {"id": "n4", "title": "Cold Frequencies", "year": 2025, "rating": 8.5,
     "description": "A radio host receives broadcasts from a winter that never ended."},
    {"id": "n5", "title": "Understudy", "year": 2024, "rating": 7.8,
     "description": "A backup actress steps into a role that starts rewriting her life."},
]


def format_rating(value):
    return f"{value:.1f}"


@component
def MovieCard(movie, on_select):
    return div(class_name="movie-card", on_click=lambda: on_select(movie),
        div(class_name="movie-card-poster",
            span(class_name="movie-card-initial", movie["title"][0]),
        ),
        div(class_name="movie-card-body",
            h4(class_name="movie-card-title", movie["title"]),
            p(class_name="movie-card-meta",
                span(class_name="movie-card-year", str(movie["year"])),
                span(class_name="movie-card-rating", f"★ {format_rating(movie['rating'])}"),
            ),
        ),
    )


@component
def MovieRow(title, movies, on_select):
    return section(class_name="movie-row",
        h3(class_name="movie-row-title", title),
        div(class_name="movie-row-track",
            *[MovieCard(key=m["id"], movie=m, on_select=on_select) for m in movies],
        ),
    )


@component
def DetailPanel(movie, on_close):
    return div(class_name="detail-panel",
        div(class_name="detail-panel-inner",
            div(class_name="detail-panel-header",
                h2(class_name="detail-panel-title", movie["title"]),
                button(class_name="detail-panel-close", on_click=lambda: on_close(), "Close"),
            ),
            p(class_name="detail-panel-meta",
                span(class_name="detail-panel-year", str(movie["year"])),
                span(class_name="detail-panel-rating", f"★ {format_rating(movie['rating'])}"),
            ),
            p(class_name="detail-panel-description", movie["description"]),
        ),
    )


@component
def MovieBrowser():
    selected, set_selected = use_state(None)

    return div(class_name="movie-browser",
        section(class_name="hero",
            div(class_name="hero-content",
                h1(class_name="hero-title", FEATURED["title"]),
                p(class_name="hero-meta",
                    span(class_name="hero-year", str(FEATURED["year"])),
                    span(class_name="hero-rating", f"★ {format_rating(FEATURED['rating'])}"),
                ),
                p(class_name="hero-description", FEATURED["description"]),
            ),
        ),
        DetailPanel(movie=selected, on_close=lambda: set_selected(None)) if selected else None,
        div(class_name="movie-rows",
            MovieRow(title="Trending", movies=TRENDING, on_select=lambda m: set_selected(m)),
            MovieRow(title="New", movies=NEW, on_select=lambda m: set_selected(m)),
        ),
    )
```
