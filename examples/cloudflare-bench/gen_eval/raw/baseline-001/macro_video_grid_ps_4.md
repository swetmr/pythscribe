<!-- exp=baseline-001 task=macro_video_grid cond=ps sample=4 model=claude-opus-4-8 cost_usd=0.041049999999999996 -->
```python
"use client"

from pyths.react import component, use_state

VIDEOS = [
    {"id": 1, "title": "Building a REST API in Rust", "channel": "CodeForge", "views": 128000, "category": "Programming"},
    {"id": 2, "title": "Lofi Beats to Study To", "channel": "ChillWave", "views": 2450000, "category": "Music"},
    {"id": 3, "title": "10 Minute Full Body Workout", "channel": "FitDaily", "views": 890000, "category": "Fitness"},
    {"id": 4, "title": "Understanding Async in JavaScript", "channel": "CodeForge", "views": 342000, "category": "Programming"},
    {"id": 5, "title": "The History of Jazz", "channel": "SoundStory", "views": 56000, "category": "Music"},
    {"id": 6, "title": "One Pan Weeknight Dinners", "channel": "HomeKitchen", "views": 731000, "category": "Cooking"},
    {"id": 7, "title": "Beginner Yoga for Flexibility", "channel": "FitDaily", "views": 415000, "category": "Fitness"},
    {"id": 8, "title": "Sourdough Bread from Scratch", "channel": "HomeKitchen", "views": 1200000, "category": "Cooking"},
    {"id": 9, "title": "Rust vs Go: A Practical Comparison", "channel": "DevInsights", "views": 267000, "category": "Programming"},
    {"id": 10, "title": "Ambient Space Music for Focus", "channel": "ChillWave", "views": 984000, "category": "Music"},
]

CATEGORIES = ["All", "Programming", "Music", "Fitness", "Cooking"]


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

    needle = query.lower().strip()
    filtered = [
        v for v in VIDEOS
        if needle in v["title"].lower()
        and (active_category == "All" or v["category"] == active_category)
    ]

    return div(class_name="video-grid",
        header(class_name="video-grid-header",
            h1("MyTube"),
            input(
                class_name="video-search",
                type="text",
                value=query,
                on_change=lambda e: set_query(e.target.value),
                placeholder="Search videos...",
            ),
        ),
        div(class_name="category-chips",
            *[button(
                key=cat,
                class_name="chip active" if cat == active_category else "chip",
                on_click=lambda cat=cat: set_active_category(cat),
                cat,
            ) for cat in CATEGORIES],
        ),
        div(class_name="video-cards") if not filtered else div(class_name="video-cards",
            *[div(key=v["id"], class_name="video-card",
                div(class_name="video-thumb"),
                h3(class_name="video-title", v["title"]),
                p(class_name="video-channel", v["channel"]),
                p(class_name="video-views", format_views(v["views"])),
            ) for v in filtered],
        ),
        p(class_name="empty-state", "No videos match your search.") if not filtered else None,
    )
```
