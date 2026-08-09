<!-- exp=baseline-haiku task=macro_movie_rows cond=ps sample=5 model=claude-haiku-4-5-20251001 cost_usd=0.0321092 -->
```python
"use client"

from pyths.react import component, use_state

@component
def MovieBrowser():
    movies_trending = [
        {"id": 1, "title": "The Matrix", "year": 1999, "rating": 8.7, "description": "A computer hacker learns about the true nature of his reality and his role in the war against its controllers."},
        {"id": 2, "title": "Inception", "year": 2010, "rating": 8.8, "description": "A skilled thief who steals corporate secrets through dream-sharing technology is given the inverse task of planting an idea."},
        {"id": 3, "title": "Interstellar", "year": 2014, "rating": 8.6, "description": "A team of explorers travel through a wormhole in space in an attempt to ensure humanity's survival."},
        {"id": 4, "title": "The Dark Knight", "year": 2008, "rating": 9.0, "description": "Batman must accept one of the greatest psychological and physical tests to fight injustice."},
        {"id": 5, "title": "Pulp Fiction", "year": 1994, "rating": 8.9, "description": "The lives of two mob hitmen, a boxer, a gangster and his wife intertwine in four tales of violence and redemption."},
    ]
    
    movies_new = [
        {"id": 6, "title": "Oppenheimer", "year": 2023, "rating": 8.3, "description": "The story of American scientist J. Robert Oppenheimer and his role in the development of the atomic bomb."},
        {"id": 7, "title": "Killers of the Flower Moon", "year": 2023, "rating": 8.2, "description": "In 1920s Oklahoma, a man puts his nephew on a path to wealth and corruption."},
        {"id": 8, "title": "Dune", "year": 2021, "rating": 8.0, "description": "Paul Atreides, a brilliant young man, travels to the most dangerous planet in the universe to ensure his family and people's future."},
        {"id": 9, "title": "Everything Everywhere All At Once", "year": 2022, "rating": 8.0, "description": "A woman receives a call and finds out her daughter is in another universe and needs her help."},
        {"id": 10, "title": "Barbie", "year": 2023, "rating": 7.4, "description": "Barbie and Ken go on an adventure in the real world upon being expelled from Barbie Land."},
    ]
    
    selected_id, set_selected_id = use_state(None)
    
    def open_detail(movie_id):
        set_selected_id(movie_id)
    
    def close_detail():
        set_selected_id(None)
    
    featured = movies_trending[0]
    
    selected_movie = None
    if selected_id:
        for m in movies_trending + movies_new:
            if m["id"] == selected_id:
                selected_movie = m
                break
    
    return div(class_name="movie-browser",
        div(class_name="hero",
            div(class_name="hero-background"),
            div(class_name="hero-content",
                h1(featured["title"]),
                p(class_name="hero-meta", f"{featured['year']} • ★ {featured['rating']}"),
                p(class_name="hero-description", featured["description"])
            )
        ),
        div(class_name="row",
            h2("Trending"),
            div(class_name="card-row",
                *[div(class_name="card", key=m["id"],
                    on_click=lambda mid=m["id"]: open_detail(mid),
                    div(class_name="card-poster"),
                    div(class_name="card-info",
                        h3(m["title"]),
                        p(class_name="card-meta", f"{m['year']} • ★ {m['rating']}")
                    )
                ) for m in movies_trending]
            )
        ),
        div(class_name="row",
            h2("New"),
            div(class_name="card-row",
                *[div(class_name="card", key=m["id"],
                    on_click=lambda mid=m["id"]: open_detail(mid),
                    div(class_name="card-poster"),
                    div(class_name="card-info",
                        h3(m["title"]),
                        p(class_name="card-meta", f"{m['year']} • ★ {m['rating']}")
                    )
                ) for m in movies_new]
            )
        ),
        (div(class_name="detail-overlay",
            on_click=close_detail,
            div(class_name="detail-panel",
                on_click=lambda e: e.stopPropagation(),
                button(class_name="detail-close", on_click=close_detail, "Close"),
                h2(class_name="detail-title", selected_movie["title"]),
                p(class_name="detail-meta", f"{selected_movie['year']} • ★ {selected_movie['rating']}"),
                p(class_name="detail-description", selected_movie["description"])
            )
        ) if selected_movie else None)
    )
```
