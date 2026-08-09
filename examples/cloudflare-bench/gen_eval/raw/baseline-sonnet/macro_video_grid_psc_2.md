<!-- exp=baseline-sonnet task=macro_video_grid cond=psc sample=2 model=claude-sonnet-5 cost_usd=0.044936699999999996 -->
```python
"use client"
R*

VIDEOS = [
    {"title": "Lo-fi Beats to Study To", "channel": "ChillHop Music", "views": "2.4M views", "category": "Music"},
    {"title": "Building a Retro Platformer in Godot", "channel": "DevByNight", "views": "184K views", "category": "Gaming"},
    {"title": "10 VS Code Extensions You Need", "channel": "CodeCraft", "views": "912K views", "category": "Tech"},
    {"title": "Speedrunning Every Mario Game", "channel": "SpeedLab", "views": "3.1M views", "category": "Gaming"},
    {"title": "One-Pan Pasta in 15 Minutes", "channel": "Simple Kitchen", "views": "540K views", "category": "Cooking"},
    {"title": "Piano Cover: Moonlight Sonata", "channel": "KeysAndStrings", "views": "1.8M views", "category": "Music"},
    {"title": "How Neural Networks Actually Work", "channel": "CodeCraft", "views": "2.9M views", "category": "Tech"},
    {"title": "Knife Skills Every Cook Should Know", "channel": "Simple Kitchen", "views": "760K views", "category": "Cooking"},
    {"title": "Live Looping Guitar Session", "channel": "ChillHop Music", "views": "398K views", "category": "Music"},
    {"title": "Top 10 Indie Games of the Year", "channel": "SpeedLab", "views": "1.1M views", "category": "Gaming"},
]

CATEGORIES = ["All", "Music", "Gaming", "Tech", "Cooking"]

def video_matches(video, query, category):
    title_ok = query.lower() in video["title"].lower()
    cat_ok = category == "All" or video["category"] == category
    return title_ok and cat_ok

@c
def VideoGrid():
    query, set_query = us("")
    category, set_category = us("All")

    filtered = [v for v in VIDEOS if video_matches(v, query, category)]

    return div(cn="video-grid-page",
        input(value=query, oh=lambda e: set_query(e.target.value),
              ph="Search videos...", cn="search-input"),
        div(cn="category-chips",
            *[button(key=c, cn=f"chip{' chip-active' if c == category else ''}",
                     oc=lambda c=c: set_category(c), c) for c in CATEGORIES]
        ),
        p(cn="empty-state", "No videos found.") if len(filtered) == 0 else div(cn="video-grid",
            *[div(key=i, cn="video-card",
                div(cn="video-thumb"),
                h3(cn="video-title", v["title"]),
                p(cn="video-channel", v["channel"]),
                p(cn="video-views", v["views"]),
            ) for i, v in enumerate(filtered)]
        ),
    )
```
