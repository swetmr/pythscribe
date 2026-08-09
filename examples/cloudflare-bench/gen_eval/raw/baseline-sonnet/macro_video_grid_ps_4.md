<!-- exp=baseline-sonnet task=macro_video_grid cond=ps sample=4 model=claude-sonnet-5 cost_usd=0.0308877 -->
```python
"use client"

from pyths.react import component, use_state

VIDEOS = [
    {"id": 1, "title": "Learning Python in 30 Minutes", "channel": "CodeWithAda", "views": 154000, "category": "Education"},
    {"id": 2, "title": "Lo-fi Beats to Relax and Study", "channel": "ChillHop Radio", "views": 980000, "category": "Music"},
    {"id": 3, "title": "Building a Gaming PC in 2026", "channel": "TechForge", "views": 452000, "category": "Technology"},
    {"id": 4, "title": "Top 10 Goals of the Season", "channel": "SportsCentral", "views": 731000, "category": "Sports"},
    {"id": 5, "title": "Easy Weeknight Pasta Recipe", "channel": "Chef Marco", "views": 210000, "category": "Cooking"},
    {"id": 6, "title": "Guitar Solo Techniques Explained", "channel": "ChillHop Radio", "views": 88000, "category": "Music"},
    {"id": 7, "title": "React Hooks Crash Course", "channel": "CodeWithAda", "views": 320000, "category": "Education"},
    {"id": 8, "title": "Morning Yoga Routine", "channel": "ZenFlow", "views": 145000, "category": "Health"},
    {"id": 9, "title": "The Future of Electric Cars", "channel": "TechForge", "views": 267000, "category": "Technology"},
    {"id": 10, "title": "Highlights: Championship Finals", "channel": "SportsCentral", "views": 1200000, "category": "Sports"},
]

def format_views(n):
    if n >= 1000000:
        return f"{n / 1000000:.1f}M views"
    if n >= 1000:
        return f"{n / 1000:.0f}K views"
    return f"{n} views"

def get_categories(videos):
    seen = []
    for v in videos:
        if v["category"] not in seen:
            seen.append(v["category"])
    return seen

@component
def VideoGrid():
    query, set_query = use_state("")
    active_category, set_active_category = use_state(None)

    categories = get_categories(VIDEOS)

    def matches(v):
        title_ok = query.lower() in v["title"].lower()
        category_ok = active_category is None or v["category"] == active_category
        return title_ok and category_ok

    filtered = [v for v in VIDEOS if matches(v)]

    def select_category(cat):
        set_active_category(None if cat == active_category else cat)

    return div(class_name="video-grid-page",
        input(
            class_name="search-input",
            type="text",
            value=query,
            placeholder="Search videos...",
            on_change=lambda e: set_query(e.target.value),
        ),
        div(class_name="category-chips",
            button(
                class_name="chip chip-active" if active_category is None else "chip",
                on_click=lambda: set_active_category(None),
                "All",
            ),
            *[button(
                key=cat,
                class_name="chip chip-active" if cat == active_category else "chip",
                on_click=lambda cat=cat: select_category(cat),
                cat,
            ) for cat in categories],
        ),
        div(class_name="empty-state", "No videos found.") if len(filtered) == 0 else div(class_name="video-grid",
            *[div(key=v["id"], class_name="video-card",
                div(class_name="video-thumbnail"),
                div(class_name="video-info",
                    h3(class_name="video-title", v["title"]),
                    p(class_name="video-channel", v["channel"]),
                    p(class_name="video-views", format_views(v["views"])),
                ),
            ) for v in filtered],
        ),
    )
```
