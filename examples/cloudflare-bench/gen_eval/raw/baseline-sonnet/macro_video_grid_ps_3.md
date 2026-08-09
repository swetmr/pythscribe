<!-- exp=baseline-sonnet task=macro_video_grid cond=ps sample=3 model=claude-sonnet-5 cost_usd=0.0325377 -->
```python
"use client"

from pyths.react import component, use_state

VIDEOS = [
    {"id": 1, "title": "Learning React in 2026", "channel": "CodeWithAda", "views": 152340, "category": "Programming"},
    {"id": 2, "title": "Lo-Fi Beats to Relax and Study", "channel": "ChillHop Radio", "views": 4820391, "category": "Music"},
    {"id": 3, "title": "10 Minute Full Body Workout", "channel": "FitFast", "views": 982103, "category": "Fitness"},
    {"id": 4, "title": "Building a REST API with Node.js", "channel": "CodeWithAda", "views": 88213, "category": "Programming"},
    {"id": 5, "title": "Top 10 Guitar Riffs of All Time", "channel": "String Theory", "views": 673500, "category": "Music"},
    {"id": 6, "title": "Morning Yoga Flow for Beginners", "channel": "FitFast", "views": 231098, "category": "Fitness"},
    {"id": 7, "title": "Understanding Async JavaScript", "channel": "DevInsights", "views": 341220, "category": "Programming"},
    {"id": 8, "title": "Piano Cover: Moonlight Sonata", "channel": "String Theory", "views": 1204983, "category": "Music"},
    {"id": 9, "title": "Street Food Tour: Bangkok", "channel": "Wander Eats", "views": 512873, "category": "Travel"},
    {"id": 10, "title": "Hidden Gems of Kyoto", "channel": "Wander Eats", "views": 723410, "category": "Travel"},
    {"id": 11, "title": "Advanced CSS Grid Techniques", "channel": "DevInsights", "views": 156732, "category": "Programming"},
    {"id": 12, "title": "HIIT Cardio Blast", "channel": "FitFast", "views": 401882, "category": "Fitness"},
]

CATEGORIES = ["All", "Programming", "Music", "Fitness", "Travel"]


def format_views(n):
    if n >= 1000000:
        return f"{n / 1000000:.1f}M views"
    if n >= 1000:
        return f"{n / 1000:.1f}K views"
    return f"{n} views"


@component
def VideoGrid():
    query, set_query = use_state("")
    active_category, set_active_category = use_state(None)

    def matches(video):
        title_ok = query.lower() in video["title"].lower()
        category_ok = active_category is None or video["category"] == active_category
        return title_ok and category_ok

    filtered = [v for v in VIDEOS if matches(v)]

    def select_category(cat):
        if cat == "All":
            set_active_category(None)
        else:
            set_active_category(cat)

    return div(class_name="video-grid-page",
        div(class_name="search-bar",
            input(
                value=query,
                on_change=lambda e: set_query(e.target.value),
                placeholder="Search videos...",
                class_name="search-input",
            ),
        ),
        div(class_name="category-chips",
            *[button(
                key=cat,
                class_name=f"chip {'chip-active' if (active_category == cat or (cat == 'All' and active_category is None)) else ''}",
                on_click=lambda cat=cat: select_category(cat),
                cat,
              ) for cat in CATEGORIES],
        ),
        div(class_name="video-grid",
            *[div(key=v["id"], class_name="video-card",
                div(class_name="video-thumb"),
                div(class_name="video-info",
                    h3(class_name="video-title", v["title"]),
                    p(class_name="video-channel", v["channel"]),
                    p(class_name="video-views", format_views(v["views"])),
                ),
              ) for v in filtered],
        ) if len(filtered) > 0 else div(class_name="empty-state",
            p("No videos match your search."),
        ),
    )
```
