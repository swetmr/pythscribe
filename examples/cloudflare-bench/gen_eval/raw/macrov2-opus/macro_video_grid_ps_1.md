<!-- exp=macrov2-opus task=macro_video_grid cond=ps sample=1 model=claude-opus-4-8 cost_usd=0.075514 -->
```python
"use client"

from pyths.react import component, use_state

VIDEOS = [
    {"id": 1, "title": "Learn PythScribe in 10 Minutes", "channel": "CodeCraft", "views": 120450, "category": "Education"},
    {"id": 2, "title": "Lofi Beats to Study To", "channel": "ChillWave", "views": 2004321, "category": "Music"},
    {"id": 3, "title": "Epic Boss Fight Compilation", "channel": "GamerZone", "views": 843210, "category": "Gaming"},
    {"id": 4, "title": "Building a REST API with Rust", "channel": "CodeCraft", "views": 55210, "category": "Education"},
    {"id": 5, "title": "Top 10 Guitar Solos of All Time", "channel": "RiffMaster", "views": 981002, "category": "Music"},
    {"id": 6, "title": "Speedrun World Record Explained", "channel": "GamerZone", "views": 412003, "category": "Gaming"},
    {"id": 7, "title": "Morning Yoga for Beginners", "channel": "ZenFlow", "views": 302150, "category": "Fitness"},
    {"id": 8, "title": "The Science of Black Holes", "channel": "SpaceLab", "views": 1523400, "category": "Education"},
    {"id": 9, "title": "HIIT Full Body Workout", "channel": "ZenFlow", "views": 210500, "category": "Fitness"},
    {"id": 10, "title": "Synthwave Mix 2026", "channel": "ChillWave", "views": 678900, "category": "Music"},
]

CATEGORIES = ["All", "Education", "Music", "Gaming", "Fitness"]


def format_views(n):
    if n >= 1_000_000:
        return f"{n / 1_000_000:.1f}M views"
    if n >= 1_000:
        return f"{n / 1_000:.1f}K views"
    return f"{n} views"


@component
def VideoGrid():
    query, set_query = use_state("")
    active_category, set_active_category = use_state("All")

    q = query.strip().lower()
    filtered = [
        v for v in VIDEOS
        if (not q or q in v["title"].lower())
        and (active_category == "All" or v["category"] == active_category)
    ]

    return div(class_name="video-grid",
        header(class_name="video-grid-header",
            h1("VideoGrid"),
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
                key=cat,
                class_name="chip chip-active" if cat == active_category else "chip",
                on_click=lambda: set_active_category(cat),
                cat,
            ) for cat in CATEGORIES]
        ),
        div(class_name="cards") if filtered else p(class_name="empty-state", "No videos match your search."),
        div(class_name="cards",
            *[div(class_name="video-card", key=v["id"],
                div(class_name="thumb"),
                h3(class_name="video-title", v["title"]),
                p(class_name="video-channel", v["channel"]),
                p(class_name="video-views", format_views(v["views"])),
            ) for v in filtered]
        ) if filtered else None,
    )
```
