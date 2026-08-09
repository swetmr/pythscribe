<!-- exp=macrov2-sonnet task=macro_video_grid cond=psc sample=5 model=claude-sonnet-5 cost_usd=0.044634 -->
```python
"use client"
R*

VIDEOS = [
    {"title": "Lo-fi Beats to Study To", "channel": "ChillWave", "views": "3.2M views", "category": "Music"},
    {"title": "Speedrunning Every Zelda Game", "channel": "PixelDash", "views": "890K views", "category": "Gaming"},
    {"title": "Building a CPU From Scratch", "channel": "LowLevel", "views": "1.1M views", "category": "Tech"},
    {"title": "10-Minute Pasta Carbonara", "channel": "KitchenFast", "views": "540K views", "category": "Cooking"},
    {"title": "Top 10 Goals of the Season", "channel": "MatchDay", "views": "2.4M views", "category": "Sports"},
    {"title": "Learn Python in One Hour", "channel": "CodeCraft", "views": "4.7M views", "category": "Education"},
    {"title": "Stand-up: My Cat Hates Me", "channel": "LaughTrack", "views": "1.9M views", "category": "Comedy"},
    {"title": "48 Hours in Tokyo", "channel": "WanderLens", "views": "3.6M views", "category": "Travel"},
    {"title": "Jazz Piano Improv Session", "channel": "ChillWave", "views": "620K views", "category": "Music"},
    {"title": "Building a Gaming PC on a Budget", "channel": "LowLevel", "views": "980K views", "category": "Tech"},
    {"title": "Marathon Training Week 1", "channel": "MatchDay", "views": "310K views", "category": "Sports"},
    {"title": "Street Food Tour: Bangkok", "channel": "WanderLens", "views": "2.8M views", "category": "Travel"},
]

def get_categories(videos):
    cats = sorted(set(v["category"] for v in videos))
    return ["All", *cats]

def video_matches(v, search, category):
    if category != "All" and v["category"] != category:
        return False
    return search.lower() in v["title"].lower()

@c
def VideoGrid():
    search, set_search = us("")
    category, set_category = us("All")

    categories = get_categories(VIDEOS)
    filtered = [v for v in VIDEOS if video_matches(v, search, category)]

    return div(cn="video-grid-app",
        input(cn="search-input", value=search,
              oh=lambda e: set_search(e.target.value),
              ph="Search videos..."),
        div(cn="chip-row",
            *[button(key=cat,
                     cn=f"chip{' active' if cat == category else ''}",
                     oc=lambda: set_category(cat),
                     cat) for cat in categories]),
        div(cn="video-grid",
            *[div(key=v["title"], cn="video-card",
                  div(cn="video-title", v["title"]),
                  div(cn="video-channel", v["channel"]),
                  div(cn="video-views", v["views"]))
              for v in filtered]) if filtered else div(cn="empty-state", "No videos found"),
    )
```
