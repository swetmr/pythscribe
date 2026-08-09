<!-- exp=macrov2-sonnet task=macro_video_grid cond=ps sample=2 model=claude-sonnet-5 cost_usd=0.029387399999999998 -->
```python
"use client"

from pyths.react import component, use_state

VIDEOS = [
    {"id": 1, "title": "Learn React in 30 Minutes", "channel": "CodeFast", "views": "1.2M views", "category": "Coding"},
    {"id": 2, "title": "Relaxing Piano Music for Study", "channel": "Calm Sounds", "views": "890K views", "category": "Music"},
    {"id": 3, "title": "Top 10 Football Goals This Week", "channel": "Sports Central", "views": "2.4M views", "category": "Sports"},
    {"id": 4, "title": "Building a REST API with Python", "channel": "CodeFast", "views": "540K views", "category": "Coding"},
    {"id": 5, "title": "Guitar Cover - Classic Rock Hits", "channel": "String Theory", "views": "310K views", "category": "Music"},
    {"id": 6, "title": "Basketball Highlights: Finals Recap", "channel": "Sports Central", "views": "1.8M views", "category": "Sports"},
    {"id": 7, "title": "How to Cook Perfect Pasta", "channel": "Kitchen Tales", "views": "670K views", "category": "Cooking"},
    {"id": 8, "title": "JavaScript Tricks Every Dev Should Know", "channel": "CodeFast", "views": "980K views", "category": "Coding"},
    {"id": 9, "title": "Baking Sourdough Bread at Home", "channel": "Kitchen Tales", "views": "420K views", "category": "Cooking"},
    {"id": 10, "title": "Lo-fi Beats to Code To", "channel": "Calm Sounds", "views": "3.1M views", "category": "Music"},
]

def get_categories(videos):
    seen = []
    for v in videos:
        if v["category"] not in seen:
            seen.append(v["category"])
    return seen


@component
def VideoCard(video):
    return div(class_name="video-card", key=video["id"],
        div(class_name="video-thumb"),
        div(class_name="video-info",
            h3(class_name="video-title", video["title"]),
            p(class_name="video-channel", video["channel"]),
            p(class_name="video-views", video["views"]),
        ),
    )


@component
def VideoGrid():
    query, set_query = use_state("")
    active_category, set_active_category = use_state(None)

    categories = get_categories(VIDEOS)

    def matches(v):
        title_match = query.strip().lower() in v["title"].lower()
        category_match = active_category is None or v["category"] == active_category
        return title_match and category_match

    filtered = [v for v in VIDEOS if matches(v)]

    def select_category(cat):
        set_active_category(None if cat == "All" else cat)

    return div(class_name="video-grid-page",
        input(
            class_name="search-input",
            value=query,
            on_change=lambda e: set_query(e.target.value),
            placeholder="Search videos...",
        ),
        div(class_name="chip-row",
            button(
                class_name="chip chip-active" if active_category is None else "chip",
                on_click=lambda: select_category("All"),
                "All",
            ),
            *[button(
                key=cat,
                class_name="chip chip-active" if active_category == cat else "chip",
                on_click=lambda cat=cat: select_category(cat),
                cat,
            ) for cat in categories],
        ),
        p(class_name="empty-state", "No videos found.") if len(filtered) == 0 else
        div(class_name="video-grid",
            *[VideoCard(video=v, key=v["id"]) for v in filtered],
        ),
    )
```
