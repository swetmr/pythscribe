<!-- exp=baseline-haiku task=macro_movie_rows cond=psc sample=3 model=claude-haiku-4-5-20251001 cost_usd=0.027568400000000003 -->
```python
"use client"
R*

movies_data = [
    {"id": 1, "title": "Cosmic Journey", "year": 2024, "rating": 8.5, "description": "An epic space adventure across distant galaxies"},
    {"id": 2, "title": "The Last Protocol", "year": 2024, "rating": 8.2, "description": "A thrilling techno-thriller about AI awakening"},
    {"id": 3, "title": "Echoes of Tomorrow", "year": 2023, "rating": 7.9, "description": "A mind-bending mystery thriller"},
    {"id": 4, "title": "Beyond the Stars", "year": 2023, "rating": 8.8, "description": "Documentary-style exploration of space exploration"},
    {"id": 5, "title": "The Silent Hour", "year": 2024, "rating": 7.6, "description": "A psychological drama about secrets and redemption"},
    {"id": 6, "title": "Neon Dreams", "year": 2023, "rating": 8.1, "description": "A cyberpunk noir set in a neon-soaked future city"},
    {"id": 7, "title": "Fractured Reality", "year": 2024, "rating": 7.8, "description": "A complex narrative about parallel worlds colliding"},
    {"id": 8, "title": "The Infinite Loop", "year": 2023, "rating": 8.4, "description": "An emotional journey through time and space"},
]

trending = movies_data[:4]
new_releases = movies_data[4:]
featured = movies_data[0]

@c
def MovieBrowser():
    selected, set_selected = us(None)
    return div(cn="movie-browser",
        HeroSection(featured),
        MovieRow("Trending", trending, selected, set_selected),
        MovieRow("New Releases", new_releases, selected, set_selected),
        DetailPanel(selected, set_selected) if selected else None,
    )

@c
def HeroSection(movie):
    return div(cn="hero-section",
        div(cn="hero-content",
            h1(movie["title"]),
            p(cn="hero-year", f"{movie['year']}"),
            p(cn="hero-rating", f"Rating: {movie['rating']}/10"),
            p(cn="hero-description", movie["description"]),
        ),
    )

@c
def MovieRow(title, movies, selected, set_selected):
    return div(cn="movie-row",
        h2(title),
        div(cn="movie-cards",
            *[MovieCard(m, selected, set_selected) for m in movies],
        ),
    )

@c
def MovieCard(movie, selected, set_selected):
    is_selected = selected and selected["id"] == movie["id"]
    return div(cn="movie-card" + (" selected" if is_selected else ""),
        oc=lambda: set_selected(movie),
        h3(movie["title"]),
        p(cn="card-year", str(movie["year"])),
        p(cn="card-rating", f"{movie['rating']}/10"),
    )

@c
def DetailPanel(movie, set_selected):
    return div(cn="detail-panel",
        div(cn="detail-content",
            h2(movie["title"]),
            p(cn="detail-year", f"Year: {movie['year']}"),
            p(cn="detail-rating", f"Rating: {movie['rating']}/10"),
            p(cn="detail-description", movie["description"]),
            button(oc=lambda: set_selected(None), cn="close-button", "Close"),
        ),
    )
```
