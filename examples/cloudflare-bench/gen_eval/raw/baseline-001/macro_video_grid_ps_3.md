<!-- exp=baseline-001 task=macro_video_grid cond=ps sample=3 model=claude-opus-4-8 cost_usd=0.042325 -->
```python
"use client"

from pyths.react import component, use_state

VIDEOS = [
    {"id": 1, "title": "Learn PythScribe in 10 Minutes", "channel": "CodeCraft", "views": 125000, "category": "Education"},
    {"id": 2, "title": "Epic Mountain Bike Fails", "channel": "RideHard", "views": 892000, "category": "Sports"},
    {"id": 3, "title": "Lo-Fi Beats for Deep Focus", "channel": "ChillWave", "views": 2400000, "category": "Music"},
    {"id": 4, "title": "Building a REST API from Scratch", "channel": "CodeCraft", "views": 54000, "category": "Education"},
    {"id": 5, "title": "The History of the Roman Empire", "channel": "PastFacts", "views": 310000, "category": "Education"},
    {"id": 6, "title": "Top 10 Goals of the Season", "channel": "RideHard", "views": 1500000, "category": "Sports"},
    {"id": 7, "title": "Acoustic Guitar Cover Session", "channel": "ChillWave", "views": 78000, "category": "Music"},
    {"id": 8, "title": "Speedrunning the Classic Platformer", "channel": "PixelPlay", "views": 640000, "category": "Gaming"},
    {"id": 9, "title": "Cozy Farming Game Playthrough", "channel": "PixelPlay", "views": 220000, "category": "Gaming"},
    {"id": 10, "title": "Cooking the Perfect Ramen", "channel": "TasteBuds", "views": 445000, "category": "Food"},
]


def format_views(n):
    if n >= 1000000:
        return f"{n / 1000000:.1f}M views"
    if n >= 1000:
        return f"{n / 1000:.0f}K views"
    return f"{n} views"


def category_list(videos):
    seen = []
    for v in videos:
        if v["category"] not in seen:
            seen.append(v["category"])
    return ["All", *seen]


@component
def VideoGrid():
    query, set_query = use_state("")
    active, set_active = use_state("All")

    categories = category_list(VIDEOS)
    q = query.strip().lower()

    filtered = [
        v for v in VIDEOS
        if (active == "All" or v["category"] == active)
        and q in v["title"].lower()
    ]

    return div(class_name="video-grid",
        header(class_name="vg-header",
            h1("Home"),
            input(
                class_name="vg-search",
                type="text",
                value=query,
                placeholder="Search videos...",
                on_change=lambda e: set_query(e.target.value),
            ),
        ),
        div(class_name="vg-chips",
            *[button(
                key=cat,
                class_name="vg-chip vg-chip-active" if cat == active else "vg-chip",
                on_click=lambda cat=cat: set_active(cat),
                cat,
            ) for cat in categories],
        ),
        div(class_name="vg-cards",
            *[article(key=v["id"], class_name="vg-card",
                div(class_name="vg-thumb"),
                h3(class_name="vg-title", v["title"]),
                p(class_name="vg-channel", v["channel"]),
                p(class_name="vg-views", format_views(v["views"])),
            ) for v in filtered],
        ) if filtered else div(class_name="vg-empty",
            p("No videos match your search."),
        ),
    )
```
