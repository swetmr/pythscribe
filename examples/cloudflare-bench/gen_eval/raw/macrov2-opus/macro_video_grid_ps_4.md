<!-- exp=macrov2-opus task=macro_video_grid cond=ps sample=4 model=claude-opus-4-8 cost_usd=0.041486999999999996 -->
```python
"use client"

from pyths.react import component, use_state

VIDEOS = [
    {"id": 1, "title": "Building a Rust Compiler from Scratch", "channel": "SystemsDojo", "views": 128000, "category": "Programming"},
    {"id": 2, "title": "Lo-fi Beats to Study To", "channel": "ChillWave", "views": 2400000, "category": "Music"},
    {"id": 3, "title": "One-Pan Garlic Butter Salmon", "channel": "HomeKitchen", "views": 543000, "category": "Cooking"},
    {"id": 4, "title": "The Physics of Black Holes Explained", "channel": "CosmosLab", "views": 987000, "category": "Science"},
    {"id": 5, "title": "React Hooks Deep Dive", "channel": "FrontendDaily", "views": 76000, "category": "Programming"},
    {"id": 6, "title": "Full Body HIIT Workout", "channel": "FitLife", "views": 310000, "category": "Fitness"},
    {"id": 7, "title": "Jazz Piano Improvisation Basics", "channel": "ChillWave", "views": 45000, "category": "Music"},
    {"id": 8, "title": "How Vaccines Actually Work", "channel": "CosmosLab", "views": 1200000, "category": "Science"},
    {"id": 9, "title": "Sourdough Bread for Beginners", "channel": "HomeKitchen", "views": 660000, "category": "Cooking"},
    {"id": 10, "title": "Morning Yoga Flow", "channel": "FitLife", "views": 88000, "category": "Fitness"},
]

CATEGORIES = ["All", "Programming", "Music", "Cooking", "Science", "Fitness"]


def format_views(n):
    if n >= 1000000:
        return f"{n / 1000000:.1f}M views"
    if n >= 1000:
        return f"{n / 1000:.0f}K views"
    return f"{n} views"


@component
def VideoCard(video):
    return div(class_name="video-card",
        div(class_name="thumbnail"),
        h3(class_name="video-title", video["title"]),
        p(class_name="video-channel", video["channel"]),
        p(class_name="video-views", format_views(video["views"])),
    )


@component
def VideoGrid():
    query, set_query = use_state("")
    category, set_category = use_state("All")

    q = query.strip().lower()
    results = [
        v for v in VIDEOS
        if (category == "All" or v["category"] == category)
        and (q == "" or q in v["title"].lower())
    ]

    return div(class_name="video-grid-page",
        div(class_name="search-bar",
            input(
                type="text",
                class_name="search-input",
                value=query,
                placeholder="Search videos...",
                on_change=lambda e: set_query(e.target.value),
            ),
        ),
        div(class_name="chip-row",
            *[button(
                key=c,
                class_name="chip active" if c == category else "chip",
                on_click=lambda: set_category(c),
                c,
            ) for c in CATEGORIES],
        ),
        div(class_name="video-grid",
            *[VideoCard(video=v, key=v["id"]) for v in results],
        ) if results else div(class_name="empty-state",
            p("No videos match your search."),
        ),
    )
```
