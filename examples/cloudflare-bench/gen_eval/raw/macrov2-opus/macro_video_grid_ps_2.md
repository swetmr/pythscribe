<!-- exp=macrov2-opus task=macro_video_grid cond=ps sample=2 model=claude-opus-4-8 cost_usd=0.040736999999999995 -->
```python
"use client"

from pyths.react import component, use_state

VIDEOS = [
    {"id": 1, "title": "Learn PythScribe in 10 Minutes", "channel": "CodeCraft", "views": 120000, "category": "Education"},
    {"id": 2, "title": "Epic Guitar Solo Compilation", "channel": "RiffMasters", "views": 890000, "category": "Music"},
    {"id": 3, "title": "Speedrunning Classic Platformers", "channel": "PixelDash", "views": 45000, "category": "Gaming"},
    {"id": 4, "title": "Building a React Grid from Scratch", "channel": "CodeCraft", "views": 210000, "category": "Education"},
    {"id": 5, "title": "Lo-Fi Beats to Study To", "channel": "ChillWave", "views": 1500000, "category": "Music"},
    {"id": 6, "title": "Top 10 Boss Fights Ever", "channel": "PixelDash", "views": 320000, "category": "Gaming"},
    {"id": 7, "title": "One-Pan Weeknight Dinners", "channel": "KitchenQuick", "views": 78000, "category": "Cooking"},
    {"id": 8, "title": "Sourdough for Absolute Beginners", "channel": "KitchenQuick", "views": 260000, "category": "Cooking"},
    {"id": 9, "title": "Debugging Async Code Live", "channel": "CodeCraft", "views": 54000, "category": "Education"},
    {"id": 10, "title": "Synthwave Driving Mix", "channel": "ChillWave", "views": 640000, "category": "Music"},
]

CATEGORIES = ["All", "Education", "Music", "Gaming", "Cooking"]


def format_views(n):
    if n >= 1000000:
        return f"{n / 1000000:.1f}M views"
    if n >= 1000:
        return f"{n / 1000:.0f}K views"
    return f"{n} views"


@component
def VideoGrid():
    query, set_query = use_state("")
    active_category, set_active_category = use_state("All")

    q = query.strip().lower()
    results = [
        v for v in VIDEOS
        if q in v["title"].lower()
        and (active_category == "All" or v["category"] == active_category)
    ]

    return div(class_name="video-grid",
        header(class_name="video-grid-header",
            h1("Home"),
            input(
                class_name="video-search",
                type="text",
                value=query,
                on_change=lambda e: set_query(e.target.value),
                placeholder="Search videos...",
            ),
        ),
        div(class_name="chip-row",
            *[button(
                key=c,
                class_name="chip active" if c == active_category else "chip",
                on_click=lambda: set_active_category(c),
                c,
              ) for c in CATEGORIES]
        ),
        div(class_name="cards") if results else p(class_name="empty-state", "No videos match your search."),
        results and div(class_name="cards",
            *[div(class_name="video-card", key=v["id"],
                div(class_name="thumbnail"),
                h3(class_name="video-title", v["title"]),
                p(class_name="video-channel", v["channel"]),
                p(class_name="video-views", format_views(v["views"])),
              ) for v in results]
        ),
    )
```
