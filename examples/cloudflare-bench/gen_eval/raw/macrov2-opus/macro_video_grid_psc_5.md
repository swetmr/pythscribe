<!-- exp=macrov2-opus task=macro_video_grid cond=psc sample=5 model=claude-opus-4-8 cost_usd=0.03854799999999999 -->
```python
"use client"
R*

VIDEOS = [
    {"id": 1, "title": "Building a REST API in Rust", "channel": "CodeForge", "views": "128K", "category": "Programming"},
    {"id": 2, "title": "Lo-Fi Beats to Study To", "channel": "ChillWave", "views": "2.1M", "category": "Music"},
    {"id": 3, "title": "The History of the Roman Empire", "channel": "PastLens", "views": "845K", "category": "History"},
    {"id": 4, "title": "10-Minute Morning Yoga", "channel": "FlowState", "views": "512K", "category": "Fitness"},
    {"id": 5, "title": "Understanding React Hooks", "channel": "CodeForge", "views": "301K", "category": "Programming"},
    {"id": 6, "title": "Jazz Piano Improvisation Tips", "channel": "ChillWave", "views": "94K", "category": "Music"},
    {"id": 7, "title": "How Volcanoes Shape the Earth", "channel": "PastLens", "views": "1.3M", "category": "Science"},
    {"id": 8, "title": "Full Body HIIT Workout", "channel": "FlowState", "views": "677K", "category": "Fitness"},
    {"id": 9, "title": "Async Rust Explained", "channel": "CodeForge", "views": "58K", "category": "Programming"},
    {"id": 10, "title": "The Science of Sleep", "channel": "PastLens", "views": "420K", "category": "Science"},
]

def categories():
    seen = []
    for v in VIDEOS:
        if v["category"] not in seen:
            seen.append(v["category"])
    return ["All", *seen]

@c
def VideoGrid():
    query, set_query = us("")
    active, set_active = us("All")

    q = query.strip().lower()
    visible = [
        v for v in VIDEOS
        if q in v["title"].lower()
        and (active == "All" or v["category"] == active)
    ]

    return div(cn="video-grid",
        div(cn="vg-search",
            input(
                cn="vg-search-input",
                value=query,
                oh=lambda e: set_query(e.target.value),
                ph="Search videos...",
            ),
        ),
        div(cn="vg-chips",
            *[button(
                key=cat,
                cn="vg-chip active" if cat == active else "vg-chip",
                oc=lambda: set_active(cat),
                cat,
              ) for cat in categories()],
        ),
        div(cn="vg-cards",
            *[div(key=v["id"], cn="vg-card",
                div(cn="vg-thumb"),
                h3(cn="vg-title", v["title"]),
                p(cn="vg-channel", v["channel"]),
                p(cn="vg-views", f"{v['views']} views"),
              ) for v in visible],
        ) if visible else div(cn="vg-empty",
            p("No videos match your search."),
        ),
    )
```
