<!-- exp=macrov2-opus task=macro_video_grid cond=psc sample=3 model=claude-opus-4-8 cost_usd=0.034498 -->
```python
"use client"
R*

VIDEOS = [
    {"id": 1, "title": "Learn PythScribe in 10 Minutes", "channel": "CodeCraft", "views": "1.2M views", "category": "Education"},
    {"id": 2, "title": "Lo-fi Beats to Relax and Study", "channel": "ChillZone", "views": "890K views", "category": "Music"},
    {"id": 3, "title": "Epic Boss Fight Compilation", "channel": "GameHub", "views": "3.4M views", "category": "Gaming"},
    {"id": 4, "title": "Cooking the Perfect Pasta", "channel": "KitchenPro", "views": "560K views", "category": "Food"},
    {"id": 5, "title": "React Hooks Deep Dive", "channel": "CodeCraft", "views": "2.1M views", "category": "Education"},
    {"id": 6, "title": "Top 10 Guitar Riffs Ever", "channel": "ChillZone", "views": "1.8M views", "category": "Music"},
    {"id": 7, "title": "Speedrun World Record Attempt", "channel": "GameHub", "views": "4.7M views", "category": "Gaming"},
    {"id": 8, "title": "Street Food Around the World", "channel": "KitchenPro", "views": "980K views", "category": "Food"},
    {"id": 9, "title": "Building a Compiler From Scratch", "channel": "CodeCraft", "views": "720K views", "category": "Education"},
]

CATEGORIES = ["All", "Education", "Music", "Gaming", "Food"]

@c
def VideoGrid():
    query, set_query = us("")
    category, set_category = us("All")

    q = query.lower()
    filtered = [
        v for v in VIDEOS
        if q in v["title"].lower()
        and (category == "All" or v["category"] == category)
    ]

    return div(cn="video-grid",
        div(cn="search-bar",
            input(
                cn="search-input",
                type="text",
                value=query,
                ph="Search videos...",
                oh=lambda e: set_query(e.target.value),
            ),
        ),
        div(cn="chip-row",
            *[button(
                key=c,
                cn=f"chip active" if c == category else "chip",
                oc=lambda c=c: set_category(c),
                c,
            ) for c in CATEGORIES],
        ),
        div(cn="cards",
            *[div(cn="video-card", key=v["id"],
                div(cn="thumbnail"),
                h3(cn="video-title", v["title"]),
                p(cn="video-channel", v["channel"]),
                p(cn="video-views", v["views"]),
            ) for v in filtered],
        ) if filtered else div(cn="empty-state",
            p("No videos match your search."),
        ),
    )
```
