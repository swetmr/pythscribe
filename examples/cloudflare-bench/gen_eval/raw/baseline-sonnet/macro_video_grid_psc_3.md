<!-- exp=baseline-sonnet task=macro_video_grid cond=psc sample=3 model=claude-sonnet-5 cost_usd=0.05360670000000001 -->
```python
"use client"
R*

VIDEOS = [
    {"title": "Learn React in 30 Minutes", "channel": "CodeMaster", "views": 1200000, "category": "Education"},
    {"title": "Epic Fail Compilation 2026", "channel": "FailArmy", "views": 5400000, "category": "Comedy"},
    {"title": "Top 10 Guitar Riffs", "channel": "MusicHub", "views": 890000, "category": "Music"},
    {"title": "Python Async Deep Dive", "channel": "CodeMaster", "views": 340000, "category": "Education"},
    {"title": "Stand-Up Special: Live in NYC", "channel": "LaughTrack", "views": 2100000, "category": "Comedy"},
    {"title": "Lo-Fi Beats to Study To", "channel": "ChillWave", "views": 15000000, "category": "Music"},
    {"title": "Building a Gaming PC 2026", "channel": "TechToday", "views": 670000, "category": "Technology"},
    {"title": "Marathon Training Tips", "channel": "FitLife", "views": 210000, "category": "Sports"},
    {"title": "React vs Vue in 2026", "channel": "TechToday", "views": 450000, "category": "Technology"},
    {"title": "World Cup Highlights", "channel": "SportsCentral", "views": 8300000, "category": "Sports"},
]

CATEGORIES = sorted({v["category"] for v in VIDEOS})

def format_views(n):
    return f"{n:,} views"

@c
def VideoGrid():
    query, set_query = us("")
    category, set_category = us(None)

    filtered = [
        v for v in VIDEOS
        if query.lower() in v["title"].lower()
        and (category is None or v["category"] == category)
    ]

    return div(cn="video-grid-page",
        input(
            value=query,
            oh=lambda e: set_query(e.target.value),
            ph="Search videos...",
            cn="search-input",
        ),
        div(cn="chip-row",
            button(
                cn=f"chip{' chip-active' if category is None else ''}",
                oc=lambda: set_category(None),
                "All",
            ),
            *[
                button(
                    key=cat,
                    cn=f"chip{' chip-active' if category == cat else ''}",
                    oc=lambda: set_category(cat),
                    cat,
                )
                for cat in CATEGORIES
            ],
        ),
        div(cn="video-grid",
            *[
                div(key=v["title"], cn="video-card",
                    div(cn="video-thumb"),
                    h3(cn="video-title", v["title"]),
                    p(cn="video-channel", v["channel"]),
                    p(cn="video-views", format_views(v["views"])),
                )
                for v in filtered
            ],
        ) if filtered else p(cn="empty-state", "No videos found"),
    )
```
