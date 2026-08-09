<!-- exp=baseline-sonnet task=macro_video_grid cond=psc sample=5 model=claude-sonnet-5 cost_usd=0.027221699999999998 -->
```python
"use client"
R*

CATEGORIES = ["All", "Music", "Gaming", "Tech", "Cooking", "Sports", "News", "Comedy"]

VIDEOS = [
    {"id": 1, "title": "Lo-fi Beats to Study To", "channel": "ChillHop Radio", "views": "2.1M views", "category": "Music"},
    {"id": 2, "title": "Speedrunning Every Mario Game", "channel": "GameTrek", "views": "890K views", "category": "Gaming"},
    {"id": 3, "title": "Building a Neural Network from Scratch", "channel": "CodeLab", "views": "540K views", "category": "Tech"},
    {"id": 4, "title": "One Pan Pasta in 15 Minutes", "channel": "Kitchen Stories", "views": "1.3M views", "category": "Cooking"},
    {"id": 5, "title": "Top 10 Goals of the Season", "channel": "SportsCenter", "views": "3.4M views", "category": "Sports"},
    {"id": 6, "title": "Breaking: Global Markets Rally", "channel": "World News Now", "views": "710K views", "category": "News"},
    {"id": 7, "title": "Stand-Up Special: Live in Chicago", "channel": "Laugh Track", "views": "980K views", "category": "Comedy"},
    {"id": 8, "title": "Acoustic Guitar Cover Session", "channel": "ChillHop Radio", "views": "420K views", "category": "Music"},
    {"id": 9, "title": "New Phone Unboxing and Review", "channel": "CodeLab", "views": "1.1M views", "category": "Tech"},
    {"id": 10, "title": "Retro Console Collection Tour", "channel": "GameTrek", "views": "300K views", "category": "Gaming"},
]

def matches(video, query, category):
    title_ok = query.lower() in video["title"].lower()
    category_ok = category == "All" or video["category"] == category
    return title_ok and category_ok

@c
def VideoGrid():
    query, set_query = us("")
    category, set_category = us("All")

    filtered = [v for v in VIDEOS if matches(v, query, category)]

    return div(cn="video-grid-page",
        input(cn="search-input", value=query, ph="Search videos...",
              oh=lambda e: set_query(e.target.value)),
        div(cn="chip-row",
            *[button(key=c, cn=f"chip{' chip-active' if c == category else ''}",
                     oc=lambda c=c: set_category(c), c)
              for c in CATEGORIES]),
        div(cn="video-grid",
            *[div(key=v["id"], cn="video-card",
                  div(cn="video-thumb"),
                  h3(cn="video-title", v["title"]),
                  p(cn="video-channel", v["channel"]),
                  p(cn="video-views", v["views"]))
              for v in filtered]) if filtered else p(cn="empty-state", "No videos found."),
    )
```
