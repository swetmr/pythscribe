<!-- exp=macrov2-sonnet task=macro_video_grid cond=ps sample=3 model=claude-sonnet-5 cost_usd=0.030482399999999996 -->
```python
"use client"
from pyths.react import component, use_state

VIDEOS = [
    {"id": 1, "title": "Learn Python in 20 Minutes", "channel": "CodeFast", "views": "1.2M views", "category": "Education"},
    {"id": 2, "title": "Lo-fi Beats to Study To", "channel": "ChillHop Radio", "views": "8.4M views", "category": "Music"},
    {"id": 3, "title": "Building a Rust Compiler From Scratch", "channel": "DevDeep", "views": "342K views", "category": "Education"},
    {"id": 4, "title": "Top 10 Goals of the Season", "channel": "SportsCenter", "views": "5.1M views", "category": "Sports"},
    {"id": 5, "title": "Epic Fails Compilation", "channel": "LOLClips", "views": "9.9M views", "category": "Comedy"},
    {"id": 6, "title": "How Black Holes Actually Work", "channel": "SpaceTime", "views": "2.7M views", "category": "Science"},
    {"id": 7, "title": "Guitar Solo Practice Routine", "channel": "ChillHop Radio", "views": "410K views", "category": "Music"},
    {"id": 8, "title": "React Hooks Explained", "channel": "CodeFast", "views": "980K views", "category": "Education"},
    {"id": 9, "title": "Marathon Training Tips", "channel": "SportsCenter", "views": "220K views", "category": "Sports"},
    {"id": 10, "title": "Stand-up Comedy Highlights", "channel": "LOLClips", "views": "3.3M views", "category": "Comedy"},
]

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

    filtered = [
        v for v in VIDEOS
        if query.lower() in v["title"].lower()
        and (active_category is None or v["category"] == active_category)
    ]

    def select_category(cat):
        set_active_category(cat)

    def clear_category():
        set_active_category(None)

    return div(class_name="video-grid-page",
        div(class_name="search-bar",
            input(
                class_name="search-input",
                type="text",
                value=query,
                placeholder="Search videos...",
                on_change=lambda e: set_query(e.target.value),
            ),
        ),
        div(class_name="category-chips",
            button(
                class_name=f"chip{' chip-active' if active_category is None else ''}",
                on_click=lambda: clear_category(),
                "All",
            ),
            *[
                button(
                    key=cat,
                    class_name=f"chip{' chip-active' if active_category == cat else ''}",
                    on_click=lambda cat=cat: select_category(cat),
                    cat,
                )
                for cat in categories
            ],
        ),
        p(class_name="empty-state", "No videos found.") if len(filtered) == 0 else
        div(class_name="video-grid",
            *[
                div(key=v["id"], class_name="video-card",
                    div(class_name="video-thumb"),
                    div(class_name="video-info",
                        h3(class_name="video-title", v["title"]),
                        p(class_name="video-channel", v["channel"]),
                        p(class_name="video-views", v["views"]),
                    ),
                )
                for v in filtered
            ],
        ),
    )
```
