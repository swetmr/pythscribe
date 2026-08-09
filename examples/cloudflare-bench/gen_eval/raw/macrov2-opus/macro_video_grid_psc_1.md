<!-- exp=macrov2-opus task=macro_video_grid cond=psc sample=1 model=claude-opus-4-8 cost_usd=0.08540899999999998 -->
```python
"use client"
R*

VIDEOS = [
    {"title": "Learn PythScribe in 10 Minutes", "channel": "CodeCraft", "views": "1.2M views", "category": "Coding"},
    {"title": "Lofi Beats to Relax and Study", "channel": "ChillWave", "views": "845K views", "category": "Music"},
    {"title": "Epic Mountain Bike Downhill Run", "channel": "TrailBlazers", "views": "2.4M views", "category": "Sports"},
    {"title": "The Science of Black Holes", "channel": "CosmosNow", "views": "980K views", "category": "Science"},
    {"title": "5 Quick Pasta Recipes", "channel": "HomeKitchen", "views": "3.1M views", "category": "Cooking"},
    {"title": "Building a REST API from Scratch", "channel": "CodeCraft", "views": "612K views", "category": "Coding"},
    {"title": "Acoustic Guitar Live Session", "channel": "ChillWave", "views": "220K views", "category": "Music"},
    {"title": "World Cup Final Highlights", "channel": "SportsCenter", "views": "8.7M views", "category": "Sports"},
    {"title": "How Vaccines Work", "channel": "CosmosNow", "views": "1.5M views", "category": "Science"},
    {"title": "Sourdough Bread for Beginners", "channel": "HomeKitchen", "views": "740K views", "category": "Cooking"},
]

CATEGORIES = ["All", "Coding", "Music", "Sports", "Science", "Cooking"]

@c
def VideoGrid():
    search, set_search = us("")
    category, set_category = us("All")

    q = search.strip().lower()
    filtered = [
        v for v in VIDEOS
        if q in v["title"].lower() and (category == "All" or v["category"] == category)
    ]

    return div(cn="video-grid",
        div(cn="vg-search",
            input(
                cn="vg-search-input",
                type="text",
                value=search,
                oh=lambda e: set_search(e.target.value),
                ph="Search videos...",
            ),
        ),
        div(cn="vg-chips",
            *[button(
                key=cat,
                cn="vg-chip active" if cat == category else "vg-chip",
                oc=lambda: set_category(cat),
                cat,
            ) for cat in CATEGORIES],
        ),
        div(cn="vg-cards",
            *[div(key=v["title"], cn="vg-card",
                div(cn="vg-thumb"),
                h3(cn="vg-title", v["title"]),
                p(cn="vg-channel", v["channel"]),
                p(cn="vg-views", v["views"]),
            ) for v in filtered],
        ) if filtered else div(cn="vg-empty", "No videos match your search."),
    )
```
