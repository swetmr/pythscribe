<!-- exp=macrov2-sonnet task=macro_video_grid cond=psc sample=4 model=claude-sonnet-5 cost_usd=0.062949 -->
```python
"use client"
R*

VIDEOS = [
    {"id": 1, "title": "Learn PythScribe in 10 Minutes", "channel": "CodeFast", "views": "1.2M views", "category": "Education"},
    {"id": 2, "title": "Lo-fi Beats to Code To", "channel": "ChillHop", "views": "3.4M views", "category": "Music"},
    {"id": 3, "title": "Building a React Grid from Scratch", "channel": "DevDocs", "views": "245K views", "category": "Education"},
    {"id": 4, "title": "Top 10 Goals of the Season", "channel": "SportsCenter", "views": "890K views", "category": "Sports"},
    {"id": 5, "title": "Synthwave Mix 2026", "channel": "ChillHop", "views": "1.8M views", "category": "Music"},
    {"id": 6, "title": "How Video Grids Actually Work", "channel": "TechExplained", "views": "512K views", "category": "Technology"},
    {"id": 7, "title": "Marathon Training Tips", "channel": "FitLife", "views": "334K views", "category": "Sports"},
    {"id": 8, "title": "Retro Gaming Documentary", "channel": "NostalgiaTV", "views": "2.1M views", "category": "Gaming"},
    {"id": 9, "title": "Speedrun World Records Explained", "channel": "NostalgiaTV", "views": "1.5M views", "category": "Gaming"},
    {"id": 10, "title": "The Physics of Video Compression", "channel": "TechExplained", "views": "678K views", "category": "Technology"},
]

CATEGORIES = []
for v in VIDEOS:
    if v["category"] not in CATEGORIES:
        CATEGORIES.append(v["category"])

@c
def VideoGrid():
    query, set_query = us("")
    category, set_category = us(None)

    filtered = [v for v in VIDEOS
                if query.lower() in v["title"].lower()
                and (category is None or v["category"] == category)]

    return div(cn="video-grid-app",
        input(value=query, oh=lambda e: set_query(e.target.value),
              ph="Search videos...", cn="search-input"),
        div(cn="chip-row",
            button(cn=f"chip {'chip-active' if category is None else ''}",
                   oc=lambda: set_category(None), "All"),
            *[button(key=c, cn=f"chip {'chip-active' if category == c else ''}",
                     oc=lambda: set_category(c), c) for c in CATEGORIES],
        ),
        div(cn="video-grid",
            *[div(key=v["id"], cn="video-card",
                  h3(cn="video-title", v["title"]),
                  p(cn="video-channel", v["channel"]),
                  p(cn="video-views", v["views"]),
              ) for v in filtered],
        ) if filtered else p(cn="empty-state", "No videos found"),
    )
```
