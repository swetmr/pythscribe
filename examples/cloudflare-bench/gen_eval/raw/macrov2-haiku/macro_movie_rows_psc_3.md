<!-- exp=macrov2-haiku task=macro_movie_rows cond=psc sample=3 model=claude-haiku-4-5-20251001 cost_usd=0.0479059 -->
```python
"use client"
R*

@c
def MovieBrowser():
    selected_movie, set_selected_movie = us(None)
    
    movies = [
        {"id": 1, "title": "Inception", "year": 2010, "rating": 8.8, "description": "A skilled thief who steals corporate secrets through dream-sharing technology."},
        {"id": 2, "title": "The Matrix", "year": 1999, "rating": 8.7, "description": "A computer hacker learns about the true nature of reality."},
        {"id": 3, "title": "Interstellar", "year": 2014, "rating": 8.6, "description": "A team of explorers travel through a wormhole in space."},
        {"id": 4, "title": "Dune", "year": 2021, "rating": 8.0, "description": "Paul Atreides travels to the dangerous planet Arrakis."},
        {"id": 5, "title": "Avatar", "year": 2009, "rating": 7.8, "description": "A marine on an alien world joins the indigenous Na'vi."},
        {"id": 6, "title": "Oppenheimer", "year": 2023, "rating": 8.4, "description": "The story of J. Robert Oppenheimer and the Manhattan Project."},
    ]
    
    featured = movies[0]
    trending = movies[1:4]
    new_releases = movies[4:6]
    
    return div(cn="movie-browser",
        div(cn="hero",
            div(cn="hero-content",
                h1(featured["title"]),
                p(cn="hero-description", featured["description"]),
                button(oc=lambda: set_selected_movie(featured), "Watch Now"),
            ),
        ),
        
        div(cn="section",
            h2("Trending Now"),
            div(cn="row",
                *[MovieCard(m, lambda: set_selected_movie(m), key=m["id"]) for m in trending],
            ),
        ),
        
        div(cn="section",
            h2("New Releases"),
            div(cn="row",
                *[MovieCard(m, lambda: set_selected_movie(m), key=m["id"]) for m in new_releases],
            ),
        ),
        
        selected_movie and DetailPanel(selected_movie, lambda: set_selected_movie(None)),
    )

@c
def MovieCard(movie, on_click):
    return div(cn="card", oc=on_click,
        div(cn="poster", "🎬"),
        div(cn="info",
            h3(movie["title"]),
            p(f"{movie['year']} • ⭐ {movie['rating']}"),
        ),
    )

@c
def DetailPanel(movie, on_close):
    return div(cn="overlay",
        div(cn="panel",
            button(cn="close-btn", oc=on_close, "×"),
            h2(movie["title"]),
            p(f"Year: {movie['year']}"),
            p(f"Rating: ⭐ {movie['rating']}/10"),
            p(cn="description", movie["description"]),
        ),
    )
```
