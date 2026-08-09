<!-- exp=baseline-sonnet task=macro_video_grid cond=ps sample=2 model=claude-sonnet-5 cost_usd=0.026687699999999998 -->
```python
"use client"

from pyths.react import component, use_state

VIDEOS = [
    {"id": 1, "title": "Learning React in 2026", "channel": "CodeCraft", "views": 152000, "category": "Tech"},
    {"id": 2, "title": "Cooking Perfect Pasta", "channel": "Chef Nico", "views": 89000, "category": "Food"},
    {"id": 3, "title": "Guitar Solo Tutorial", "channel": "Music Mentor", "views": 43000, "category": "Music"},
    {"id": 4, "title": "React Hooks Deep Dive", "channel": "CodeCraft", "views": 210000, "category": "Tech"},
    {"id": 5, "title": "Morning Yoga Routine", "channel": "Flex Daily", "views": 67000, "category": "Fitness"},
    {"id": 6, "title": "Street Food Tour Tokyo", "channel": "Chef Nico", "views": 305000, "category": "Food"},
    {"id": 7, "title": "Piano Cover - Nocturne", "channel": "Music Mentor", "views": 21000, "category": "Music"},
    {"id": 8, "title": "Full Body Home Workout", "channel": "Flex Daily", "views": 128000, "category": "Fitness"},
    {"id": 9, "title": "JavaScript Tips and Tricks", "channel": "CodeCraft", "views": 98000, "category": "Tech"},
    {"id": 10, "title": "Baking Sourdough Bread", "channel": "Chef Nico", "views": 54000, "category": "Food"},
]

CATEGORIES = ["All", "Tech", "Food", "Music", "Fitness"]


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

    def matches(video):
        title_match = query.lower() in video["title"].lower()
        category_match = active_category == "All" or video["category"] == active_category
        return title_match and category_match

    filtered = [v for v in VIDEOS if matches(v)]

    def select_category(cat):
        set_active_category(cat)

    return div(class_name="video-grid-page",
        input(
            class_name="search-input",
            value=query,
            on_change=lambda e: set_query(e.target.value),
            placeholder="Search videos...",
        ),
        div(class_name="chip-row",
            *[button(
                key=cat,
                class_name=f"chip active" if cat == active_category else "chip",
                on_click=lambda cat=cat: select_category(cat),
                cat,
            ) for cat in CATEGORIES],
        ),
        p(class_name="empty-state", "No videos found.") if len(filtered) == 0 else div(class_name="video-grid",
            *[div(key=v["id"], class_name="video-card",
                div(class_name="video-thumb"),
                h3(class_name="video-title", v["title"]),
                p(class_name="video-channel", v["channel"]),
                p(class_name="video-views", format_views(v["views"])),
            ) for v in filtered],
        ),
    )
```
