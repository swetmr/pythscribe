<!-- exp=baseline-sonnet task=macro_video_grid cond=psc sample=1 model=claude-sonnet-5 cost_usd=0.077157 -->
```python
"use client"
R*

VIDEOS = [
    {"id": 1, "title": "Lo-fi Beats to Study To", "channel": "Chillhop Music", "views": 1200000, "category": "Music"},
    {"id": 2, "title": "Speedrunning Any% World Record", "channel": "GameFast", "views": 850000, "category": "Gaming"},
    {"id": 3, "title": "Building a REST API in an Hour", "channel": "DevDaily", "views": 430000, "category": "Tech"},
    {"id": 4, "title": "Perfect Pasta from Scratch", "channel": "Kitchen Basics", "views": 670000, "category": "Cooking"},
    {"id": 5, "title": "Top 10 Music Production Tips", "channel": "Chillhop Music", "views": 320000, "category": "Music"},
    {"id": 6, "title": "React Hooks Explained", "channel": "DevDaily", "views": 980000, "category": "Tech"},
    {"id": 7, "title": "Boss Fight Compilation", "channel": "GameFast", "views": 1500000, "category": "Gaming"},
    {"id": 8, "title": "5-Minute Breakfast Ideas", "channel": "Kitchen Basics", "views": 210000, "category": "Cooking"},
    {"id": 9, "title": "Synthwave Mix Vol. 3", "channel": "Chillhop Music", "views": 540000, "category": "Music"},
    {"id": 10, "title": "Debugging Like a Pro", "channel": "DevDaily", "views": 150000, "category": "Tech"},
]

CATEGORIES = ["All", "Music", "Gaming", "Tech", "Cooking"]

def format_views(n):
    if n >= 1000000:
        return f"{n / 1000000:.1f}M views"
    if n >= 1000:
        return f"{n / 1000:.0f}K views"
    return f"{n} views"

@c
def VideoGrid():
    query, set_query = us("")
    category, set_category = us("All")

    filtered = [
        v for v in VIDEOS
        if query.lower() in v["title"].lower()
        and (category == "All" or v["category"] == category)
    ]

    return div(cn="video-grid-page",
        input(cn="search-input", value=query, ph="Search videos...",
              oh=lambda e: set_query(e.target.value)),
        div(cn="chip-row",
            *[button(key=cat, cn=f"chip{' chip-active' if cat == category else ''}",
                      oc=lambda cat=cat: set_category(cat), cat)
              for cat in CATEGORIES],
        ),
        div(cn="video-grid",
            *[div(key=v["id"], cn="video-card",
                div(cn="video-thumb"),
                div(cn="video-info",
                    h3(cn="video-title", v["title"]),
                    p(cn="video-channel", v["channel"]),
                    p(cn="video-views", format_views(v["views"])),
                ),
            ) for v in filtered],
        ) if filtered else div(cn="empty-state", "No videos found."),
    )
```
