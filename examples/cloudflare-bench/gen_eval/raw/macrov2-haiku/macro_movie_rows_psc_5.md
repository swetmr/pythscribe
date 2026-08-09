<!-- exp=macrov2-haiku task=macro_movie_rows cond=psc sample=5 model=claude-haiku-4-5-20251001 cost_usd=0.0426909 -->
```python
"use client"
R*

@c
def MovieBrowser():
    trending = [
        {"title": "Inception", "year": 2010, "rating": 8.8, "description": "A skilled thief who steals corporate secrets through dream-sharing technology."},
        {"title": "The Matrix", "year": 1999, "rating": 8.7, "description": "A computer hacker learns about the true nature of his reality."},
        {"title": "Interstellar", "year": 2014, "rating": 8.6, "description": "A team of explorers travel through a wormhole to save humanity."},
        {"title": "The Dark Knight", "year": 2008, "rating": 9.0, "description": "Batman faces his greatest challenge: the Joker."},
    ]
    
    new_movies = [
        {"title": "Dune", "year": 2021, "rating": 8.0, "description": "Paul Atreides must travel to the dangerous planet Dune."},
        {"title": "Oppenheimer", "year": 2023, "rating": 8.5, "description": "The story of J. Robert Oppenheimer and the atomic bomb."},
        {"title": "Barbie", "year": 2023, "rating": 7.9, "description": "Barbie and Ken navigate the real world on a new adventure."},
        {"title": "Killers of the Flower Moon", "year": 2023, "rating": 8.2, "description": "A story of greed and murder in the American West."},
    ]
    
    selected_movie, set_selected_movie = us(None)
    featured = trending[0]
    
    @c
    def MovieCard(movie):
        return div(cn="movie-card", oc=lambda: set_selected_movie(movie),
            div(cn="card-image"),
            div(cn="card-title", movie["title"]),
            div(cn="card-meta", f"{movie['year']} · {movie['rating']}/10"),
        )
    
    @c
    def DetailPanel(movie):
        return div(cn="detail-overlay", oc=lambda: set_selected_movie(None),
            div(cn="detail-panel", oc=lambda e: e.stopPropagation(),
                div(cn="detail-header",
                    h2(movie["title"]),
                    button(cn="close-btn", oc=lambda: set_selected_movie(None), "×"),
                ),
                div(cn="detail-content",
                    p(f"Year: {movie['year']}"),
                    p(f"Rating: {movie['rating']}/10"),
                    p(movie["description"]),
                ),
            ),
        )
    
    return div(cn="movie-browser",
        div(cn="hero-section",
            div(cn="hero-content",
                h1(featured["title"]),
                p(featured["description"]),
                p(f"Year: {featured['year']} | Rating: {featured['rating']}/10"),
            ),
        ),
        div(cn="browse-row",
            h2("Trending Now"),
            div(cn="row-content",
                *[MovieCard(movie) for movie in trending],
            ),
        ),
        div(cn="browse-row",
            h2("New Releases"),
            div(cn="row-content",
                *[MovieCard(movie) for movie in new_movies],
            ),
        ),
        DetailPanel(selected_movie) if selected_movie else None,
    )
```
