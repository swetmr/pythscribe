<!-- exp=macrov2-sonnet task=macro_video_grid cond=psc sample=3 model=claude-sonnet-5 cost_usd=0.025854000000000002 -->
```python
"use client"
R*

VIDEOS = [
    {"id": 1, "title": "Learning React Hooks in 10 Minutes", "channel": "CodeCraft", "views": "1.2M views", "category": "Tech"},
    {"id": 2, "title": "Full Marathon Training Plan", "channel": "RunStrong", "views": "340K views", "category": "Sports"},
    {"id": 3, "title": "Lo-Fi Beats to Study To", "channel": "ChillWave", "views": "5.8M views", "category": "Music"},
    {"id": 4, "title": "Building a Startup From Scratch", "channel": "FounderTalks", "views": "89K views", "category": "Business"},
    {"id": 5, "title": "Top 10 Python Tricks", "channel": "CodeCraft", "views": "2.4M views", "category": "Tech"},
    {"id": 6, "title": "World Cup Highlights 2026", "channel": "SportsCentral", "views": "12M views", "category": "Sports"},
    {"id": 7, "title": "Guitar Solo Compilation", "channel": "ChillWave", "views": "670K views", "category": "Music"},
    {"id": 8, "title": "How I Raised My First Round", "channel": "FounderTalks", "views": "150K views", "category": "Business"},
    {"id": 9, "title": "Cooking Pasta Like a Pro", "channel": "TastyBites", "views": "980K views", "category": "Food"},
    {"id": 10, "title": "5-Minute Vegan Breakfast", "channel": "TastyBites", "views": "410K views", "category": "Food"},
]

CATEGORIES = ["All", "Tech", "Sports", "Music", "Business", "Food"]

@c
def VideoGrid():
    query, set_query = us("")
    category, set_category = us("All")

    def matches(v):
        title_ok = query.lower() in v["title"].lower()
        cat_ok = category == "All" or v["category"] == category
        return title_ok and cat_ok

    filtered = [v for v in VIDEOS if matches(v)]

    def select_category(c):
        return lambda: set_category(c)

    return div(cn="video-grid-page",
        input(cn="search-input", value=query, ph="Search videos...",
              oh=lambda e: set_query(e.target.value)),
        div(cn="chip-row",
            *[button(key=c, cn=f"chip {'chip-active' if c == category else ''}",
                     oc=select_category(c), c)
              for c in CATEGORIES]),
        div(cn="video-grid",
            *[div(key=v["id"], cn="video-card",
                  div(cn="video-thumb"),
                  div(cn="video-info",
                      h3(cn="video-title", v["title"]),
                      p(cn="video-channel", v["channel"]),
                      p(cn="video-views", v["views"]),
                  ))
              for v in filtered]),
        (len(filtered) == 0) and p(cn="empty-state", "No videos found."),
    )
```
