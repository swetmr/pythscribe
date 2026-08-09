<!-- exp=macrov2-opus task=macro_video_grid cond=ps sample=3 model=claude-opus-4-8 cost_usd=0.041087 -->
```python
"use client"

from pyths.react import component, use_state

VIDEOS = [
    {"id": 1, "title": "Building a REST API in Rust", "channel": "CodeForge", "views": 128000, "category": "Programming"},
    {"id": 2, "title": "Lo-Fi Beats to Study To", "channel": "ChillWave", "views": 2400000, "category": "Music"},
    {"id": 3, "title": "10 Minute Morning Yoga", "channel": "FlowState", "views": 543000, "category": "Fitness"},
    {"id": 4, "title": "The Physics of Black Holes", "channel": "CosmosLab", "views": 987000, "category": "Science"},
    {"id": 5, "title": "One Pan Weeknight Pasta", "channel": "KitchenKit", "views": 76000, "category": "Cooking"},
    {"id": 6, "title": "Understanding React Hooks", "channel": "CodeForge", "views": 312000, "category": "Programming"},
    {"id": 7, "title": "Ambient Synth Soundscapes", "channel": "ChillWave", "views": 45000, "category": "Music"},
    {"id": 8, "title": "Full Body HIIT Workout", "channel": "FlowState", "views": 1200000, "category": "Fitness"},
    {"id": 9, "title": "How Vaccines Work", "channel": "CosmosLab", "views": 654000, "category": "Science"},
    {"id": 10, "title": "Knife Skills for Beginners", "channel": "KitchenKit", "views": 89000, "category": "Cooking"},
]


def format_views(n):
    if n >= 1000000:
        return f"{n / 1000000:.1f}M views"
    if n >= 1000:
        return f"{n / 1000:.0f}K views"
    return f"{n} views"


def categories():
    seen = []
    for v in VIDEOS:
        if v["category"] not in seen:
            seen.append(v["category"])
    return ["All", *seen]


@component
def VideoGrid():
    query, set_query = use_state("")
    active, set_active = use_state("All")

    q = query.strip().lower()
    matches = [
        v for v in VIDEOS
        if q in v["title"].lower() and (active == "All" or v["category"] == active)
    ]

    return div(class_name="video-grid",
        div(class_name="vg-toolbar",
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
                on_click=lambda: set_active(cat),
                cat,
            ) for cat in categories()],
        ),
        div(class_name="vg-cards",
            *[div(key=v["id"], class_name="vg-card",
                div(class_name="vg-thumb"),
                h3(class_name="vg-title", v["title"]),
                p(class_name="vg-channel", v["channel"]),
                p(class_name="vg-views", format_views(v["views"])),
            ) for v in matches],
        ) if matches else div(class_name="vg-empty",
            p("No videos match your search."),
        ),
    )
```
