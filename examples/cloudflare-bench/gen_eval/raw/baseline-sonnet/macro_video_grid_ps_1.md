<!-- exp=baseline-sonnet task=macro_video_grid cond=ps sample=1 model=claude-sonnet-5 cost_usd=0.048012 -->
```python
"use client"

from pyths.react import component, use_state

VIDEOS = [
    {"id": 1, "title": "Learn React in 30 Minutes", "channel": "CodeMaster", "views": "1.2M views", "category": "Education"},
    {"id": 2, "title": "Lo-fi Beats to Study To", "channel": "ChillSounds", "views": "8.5M views", "category": "Music"},
    {"id": 3, "title": "Top 10 Goals of the Season", "channel": "SportsCentral", "views": "3.4M views", "category": "Sports"},
    {"id": 4, "title": "Building a Gaming PC in 2026", "channel": "TechBuilds", "views": "745K views", "category": "Technology"},
    {"id": 5, "title": "Easy Pasta Recipe for Beginners", "channel": "HomeCookHub", "views": "980K views", "category": "Cooking"},
    {"id": 6, "title": "Advanced Python Tips", "channel": "CodeMaster", "views": "512K views", "category": "Education"},
    {"id": 7, "title": "Guitar Cover - Acoustic Sessions", "channel": "ChillSounds", "views": "2.1M views", "category": "Music"},
    {"id": 8, "title": "Basketball Highlights Weekly", "channel": "SportsCentral", "views": "1.8M views", "category": "Sports"},
    {"id": 9, "title": "Smartphone Review: Latest Flagship", "channel": "TechBuilds", "views": "623K views", "category": "Technology"},
    {"id": 10, "title": "Baking Bread from Scratch", "channel": "HomeCookHub", "views": "1.1M views", "category": "Cooking"},
]

def get_categories(videos):
    seen = []
    for v in videos:
        if v["category"] not in seen:
            seen.append(v["category"])
    return seen


@component
def VideoGrid():
    search, set_search = use_state("")
    active_category, set_active_category = use_state(None)

    categories = get_categories(VIDEOS)

    query = search.strip().lower()
    filtered = [
        v for v in VIDEOS
        if query in v["title"].lower()
        and (active_category is None or v["category"] == active_category)
    ]

    def select_category(cat):
        set_active_category(cat)

    def clear_category():
        set_active_category(None)

    return div(class_name="video-grid-page",
        div(class_name="video-search-bar",
            input(
                class_name="video-search-input",
                type="text",
                placeholder="Search videos...",
                value=search,
                on_change=lambda e: set_search(e.target.value),
            ),
        ),
        div(class_name="category-chip-row",
            button(
                class_name=f"category-chip{' active' if active_category is None else ''}",
                on_click=lambda: clear_category(),
                "All",
            ),
            *[
                button(
                    key=cat,
                    class_name=f"category-chip{' active' if cat == active_category else ''}",
                    on_click=lambda cat=cat: select_category(cat),
                    cat,
                )
                for cat in categories
            ],
        ),
        div(class_name="video-grid",
            *[
                div(key=v["id"], class_name="video-card",
                    div(class_name="video-thumbnail-placeholder"),
                    div(class_name="video-card-info",
                        h3(class_name="video-title", v["title"]),
                        p(class_name="video-channel", v["channel"]),
                        p(class_name="video-views", v["views"]),
                    ),
                )
                for v in filtered
            ],
        ) if len(filtered) > 0 else div(class_name="empty-state",
            p("No videos found. Try a different search or category."),
        ),
    )
```
