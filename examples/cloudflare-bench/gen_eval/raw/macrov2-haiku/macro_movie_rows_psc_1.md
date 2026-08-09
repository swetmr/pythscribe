<!-- exp=macrov2-haiku task=macro_movie_rows cond=psc sample=1 model=claude-haiku-4-5-20251001 cost_usd=0.04827360000000001 -->
```python
"use client"
R*

@c
def MovieBrowser():
    movies_trending = [
        {"id": 1, "title": "Inception", "year": 2010, "rating": 8.8, "desc": "A skilled thief who steals corporate secrets through dream-sharing technology."},
        {"id": 2, "title": "The Dark Knight", "year": 2008, "rating": 9.0, "desc": "When the menace known as the Joker wreaks havoc on Gotham, Batman must confront his darkest nemesis."},
        {"id": 3, "title": "Interstellar", "year": 2014, "rating": 8.6, "desc": "A team of explorers travel through a wormhole in space in an attempt to ensure humanity's survival."},
        {"id": 4, "title": "The Matrix", "year": 1999, "rating": 8.7, "desc": "A computer hacker learns about the true nature of reality and his role in the war against its controllers."},
    ]
    
    movies_new = [
        {"id": 5, "title": "Dune", "year": 2021, "rating": 8.0, "desc": "Paul Atreides, a brilliant young man, must travel to the dangerous planet Dune to ensure the future of his family."},
        {"id": 6, "title": "Oppenheimer", "year": 2023, "rating": 8.5, "desc": "The story of American scientist J. Robert Oppenheimer and his role in the development of the atomic bomb."},
        {"id": 7, "title": "Barbie", "year": 2023, "rating": 7.9, "desc": "Barbie's perfect life in the Mattel present is disrupted when she gains consciousness and emotions."},
        {"id": 8, "title": "Killers of the Flower Moon", "year": 2023, "rating": 8.2, "desc": "When oil is discovered beneath their land, the Osage Nation becomes the target of a series of murders."},
    ]
    
    featured = movies_trending[0]
    selected_id, set_selected_id = us(None)
    
    def open_detail(movie_id):
        set_selected_id(movie_id)
    
    def close_detail():
        set_selected_id(None)
    
    selected_movie = next((m for m in movies_trending + movies_new if m["id"] == selected_id), None)
    
    @c
    def MovieCard(movie):
        return div(cn="movie-card", oc=lambda: open_detail(movie["id"]),
            h3(movie["title"]),
            p(f"{movie['year']} • {movie['rating']}/10"),
        )
    
    return div(cn="movie-browser",
        div(cn="hero",
            h1(featured["title"]),
            p(featured["desc"]),
        ),
        div(cn="section",
            h2("Trending"),
            div(cn="row",
                *[MovieCard(m) for m in movies_trending]
            ),
        ),
        div(cn="section",
            h2("New Releases"),
            div(cn="row",
                *[MovieCard(m) for m in movies_new]
            ),
        ),
        (div(cn="detail-panel",
            div(cn="panel-content",
                h2(selected_movie["title"]),
                p(f"Year: {selected_movie['year']}"),
                p(f"Rating: {selected_movie['rating']}/10"),
                p(selected_movie["desc"]),
                button(oc=lambda: close_detail(), "Close"),
            ),
        ) if selected_movie else None),
    )
```
