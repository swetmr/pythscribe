<!-- exp=macrov2-sonnet task=macro_video_grid cond=psc sample=2 model=claude-sonnet-5 cost_usd=0.059709 -->
```python
"use client"
R*

VIDEOS = [
    {"title": "Lo-fi Beats to Study To", "channel": "ChillHop Radio", "views": 1250000, "category": "Music"},
    {"title": "Speedrunning Every Mario Game", "channel": "GameWizard", "views": 890000, "category": "Gaming"},
    {"title": "Building a CPU From Scratch", "channel": "TechDeepDive", "views": 430000, "category": "Tech"},
    {"title": "One Pan Pasta in 10 Minutes", "channel": "QuickBites", "views": 275000, "category": "Cooking"},
    {"title": "Top 10 Goals of the Season", "channel": "SportsCentral", "views": 610000, "category": "Sports"},
    {"title": "Breaking: Market Update Today", "channel": "DailyNews Now", "views": 95000, "category": "News"},
    {"title": "Piano Cover - Moonlight Sonata", "channel": "ChillHop Radio", "views": 320000, "category": "Music"},
    {"title": "React Hooks Explained Fast", "channel": "TechDeepDive", "views": 512000, "category": "Tech"},
    {"title": "Retro Gaming Console Teardown", "channel": "GameWizard", "views": 180000, "category": "Gaming"},
    {"title": "5 Baking Mistakes Beginners Make", "channel": "QuickBites", "views": 145000, "category": "Cooking"},
]

CATEGORIES = sorted(set(v["category"] for v in VIDEOS))

def format_views(n):
    return f"{n:,} views"

@c
def VideoGrid():
    query, set_query = us("")
    active_cat, set_active_cat = us(None)

    filtered = [
        v for v in VIDEOS
        if query.lower() in v["title"].lower()
        and (active_cat is None or v["category"] == active_cat)
    ]

    return div(cn="video-grid-page",
        div(cn="search-bar",
            input(value=query, oh=lambda e: set_query(e.target.value),
                  ph="Search videos...", type="text"),
        ),
        div(cn="chips-row",
            button(cn=f"chip{' chip-active' if active_cat is None else ''}",
                   oc=lambda: set_active_cat(None), "All"),
            *[button(key=cat, cn=f"chip{' chip-active' if active_cat == cat else ''}",
                     oc=lambda: set_active_cat(cat), cat)
              for cat in CATEGORIES],
        ),
        div(cn="video-grid",
            *[div(key=v["title"], cn="video-card",
                  div(cn="video-thumb"),
                  div(cn="video-info",
                      h3(cn="video-title", v["title"]),
                      p(cn="video-channel", v["channel"]),
                      p(cn="video-views", format_views(v["views"])),
                  ),
              ) for v in filtered]
        ) if filtered else p(cn="empty-state", "No videos found."),
    )
```
