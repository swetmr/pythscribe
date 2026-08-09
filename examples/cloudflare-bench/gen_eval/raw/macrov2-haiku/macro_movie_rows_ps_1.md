<!-- exp=macrov2-haiku task=macro_movie_rows cond=ps sample=1 model=claude-haiku-4-5-20251001 cost_usd=0.0915786 -->
```python
"use client"

from pyths.react import component, use_state

@component
def MovieBrowser():
    trending_movies = [
        {
            "id": 1,
            "title": "Dune: Part Two",
            "year": 2024,
            "rating": 8.5,
            "description": "Paul Atreides travels to the dangerous planet Arrakis to ensure the future of his family and people.",
        },
        {
            "id": 2,
            "title": "The Brutalist",
            "year": 2023,
            "rating": 8.2,
            "description": "A Hungarian-Jewish businessman rebuilds his life in post-war America.",
        },
        {
            "id": 3,
            "title": "Oppenheimer",
            "year": 2023,
            "rating": 8.4,
            "description": "The story of J. Robert Oppenheimer and the atomic bomb.",
        },
        {
            "id": 4,
            "title": "Killers of the Flower Moon",
            "year": 2023,
            "rating": 8.0,
            "description": "A mafia family's involvement in the murder of wealthy Osage Nation members.",
        },
    ]
    
    new_movies = [
        {
            "id": 5,
            "title": "Poor Things",
            "year": 2024,
            "rating": 7.8,
            "description": "A woman brought back to life by a mad scientist explores the world.",
        },
        {
            "id": 6,
            "title": "American Fiction",
            "year": 2023,
            "rating": 7.9,
            "description": "A novelist's life unfolds in unexpected ways.",
        },
        {
            "id": 7,
            "title": "Anatomy of a Fall",
            "year": 2023,
            "rating": 7.7,
            "description": "A woman is accused of murdering her husband.",
        },
        {
            "id": 8,
            "title": "Past Lives",
            "year": 2023,
            "rating": 7.6,
            "description": "Two childhood friends are reunited by chance.",
        },
    ]
    
    selected_movie, set_selected_movie = use_state(None)
    featured = trending_movies[0]
    
    return div(class_name="movie-browser",
        div(class_name="hero-section",
            div(class_name="hero-content",
                h1(class_name="hero-title", featured["title"]),
                p(class_name="hero-year", f"Released {featured['year']}"),
                p(class_name="hero-description", featured["description"]),
                button(class_name="hero-button", on_click=lambda: set_selected_movie(featured),
                       "View Details"),
            ),
        ),
        
        div(class_name="row",
            h2(class_name="row-title", "Trending Now"),
            div(class_name="movie-row",
                *[
                    div(*[
                        div(class_name="movie-poster", "🎬"),
                        h3(class_name="movie-title", m["title"]),
                        p(class_name="movie-year", str(m["year"])),
                        p(class_name="movie-rating", f"⭐ {m['rating']}/10"),
                    ], key=m["id"], class_name="movie-card",
                       on_click=lambda: set_selected_movie(m))
                    for m in trending_movies
                ]
            ),
        ),
        
        div(class_name="row",
            h2(class_name="row-title", "New Releases"),
            div(class_name="movie-row",
                *[
                    div(*[
                        div(class_name="movie-poster", "✨"),
                        h3(class_name="movie-title", m["title"]),
                        p(class_name="movie-year", str(m["year"])),
                        p(class_name="movie-rating", f"⭐ {m['rating']}/10"),
                    ], key=m["id"], class_name="movie-card",
                       on_click=lambda: set_selected_movie(m))
                    for m in new_movies
                ]
            ),
        ),
        
        (
            div(*[
                div(*[
                    button(class_name="close-button", on_click=lambda: set_selected_movie(None), "✕"),
                    h2(class_name="detail-title", selected_movie["title"]),
                    p(class_name="detail-year", f"Released {selected_movie['year']}"),
                    p(class_name="detail-rating", f"Rating: ⭐ {selected_movie['rating']}/10"),
                    p(class_name="detail-description", selected_movie["description"]),
                ], class_name="detail-panel", on_click=lambda e: e.stopPropagation()),
            ], class_name="modal-overlay", on_click=lambda: set_selected_movie(None))
        ) if selected_movie else None,
    )
```
