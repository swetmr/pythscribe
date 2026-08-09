<!-- exp=macrov2-haiku task=macro_movie_rows cond=psc sample=4 model=claude-haiku-4-5-20251001 cost_usd=0.0522759 -->
```python
"use client"
R*

movies_trending = [
    {
        "id": 1,
        "title": "Inception",
        "year": 2010,
        "rating": 8.8,
        "description": "A skilled thief who specializes in extraction from dreams must perform the impossible: inception."
    },
    {
        "id": 2,
        "title": "Interstellar",
        "year": 2014,
        "rating": 8.6,
        "description": "A team of astronauts travel through a wormhole in space to ensure humanity's survival."
    },
    {
        "id": 3,
        "title": "The Dark Knight",
        "year": 2008,
        "rating": 9.0,
        "description": "Batman faces a new criminal mastermind known only as the Joker, a man of no empathy."
    },
    {
        "id": 4,
        "title": "Parasite",
        "year": 2019,
        "rating": 8.6,
        "description": "Greed and class discrimination threaten the newly formed symbiotic relationship between two families."
    },
    {
        "id": 5,
        "title": "Dune",
        "year": 2021,
        "rating": 8.0,
        "description": "Paul Atreides, a brilliant young man, travels to the dangerous planet Dune to ensure the future of his family."
    },
]

movies_new = [
    {
        "id": 6,
        "title": "Oppenheimer",
        "year": 2023,
        "rating": 8.4,
        "description": "The story of American scientist J. Robert Oppenheimer and his role in the development of the atomic bomb."
    },
    {
        "id": 7,
        "title": "Killers of the Flower Moon",
        "year": 2023,
        "rating": 8.1,
        "description": "An investigation into a series of murders of wealthy Osage Native Americans in 1920s Oklahoma."
    },
    {
        "id": 8,
        "title": "Barbie",
        "year": 2023,
        "rating": 7.9,
        "description": "Barbie and Ken leave Barbie Land to come to the real world and discover what it's like to be human."
    },
    {
        "id": 9,
        "title": "Poor Things",
        "year": 2023,
        "rating": 7.7,
        "description": "A young woman brought back to life by an eccentric scientist embarks on a bold and daring exploration of the world."
    },
    {
        "id": 10,
        "title": "The Iron Claw",
        "year": 2023,
        "rating": 8.2,
        "description": "The rise and tragic fall of the legendary professional wrestling Von Erich family of Dallas."
    },
]

@c
def MovieCard(movie, on_select):
    return div(cn="movie-card", oc=on_select,
        div(cn="card-image"),
        div(cn="card-info",
            h3(movie["title"]),
            p(f"{movie['year']} • ★ {movie['rating']}")
        )
    )

@c
def DetailPanel(movie, on_close):
    def stop_propagation(e):
        e.stopPropagation()
    
    return div(cn="detail-overlay", oc=on_close,
        div(cn="detail-panel", oc=stop_propagation,
            div(cn="detail-header",
                h2(movie["title"]),
                button(cn="close-btn", oc=on_close, "×")
            ),
            div(cn="detail-body",
                p(f"Year: {movie['year']}"),
                p(f"Rating: ★ {movie['rating']}"),
                p(movie["description"])
            )
        )
    )

@c
def MovieBrowser():
    selected_movie, set_selected = us(None)
    featured = movies_trending[0]
    
    return div(cn="movie-browser",
        div(cn="hero",
            h1(featured["title"]),
            p(featured["description"]),
            p(f"{featured['year']} • ★ {featured['rating']}")
        ),
        div(cn="rows",
            section(cn="row",
                h2("Trending"),
                div(cn="card-row",
                    *[MovieCard(key=m["id"], movie=m, on_select=lambda: set_selected(m)) 
                      for m in movies_trending]
                )
            ),
            section(cn="row",
                h2("New"),
                div(cn="card-row",
                    *[MovieCard(key=m["id"], movie=m, on_select=lambda: set_selected(m)) 
                      for m in movies_new]
                )
            )
        ),
        selected_movie and DetailPanel(movie=selected_movie, on_close=lambda: set_selected(None))
    )
```
