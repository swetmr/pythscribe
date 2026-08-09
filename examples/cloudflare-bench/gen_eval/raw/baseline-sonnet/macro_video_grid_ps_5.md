<!-- exp=baseline-sonnet task=macro_video_grid cond=ps sample=5 model=claude-sonnet-5 cost_usd=0.0300477 -->
```python
"use client"

from pyths.react import component, use_state

VIDEOS = [
    {"id": 1, "title": "Learn React in 30 Minutes", "channel": "CodeWithMe", "views": "1.2M views", "category": "Education"},
    {"id": 2, "title": "Lo-Fi Beats to Study To", "channel": "ChillHop Radio", "views": "8.4M views", "category": "Music"},
    {"id": 3, "title": "Top 10 Football Goals This Week", "channel": "SportsCenter", "views": "540K views", "category": "Sports"},
    {"id": 4, "title": "Building a Gaming PC in 2026", "channel": "TechBuilds", "views": "2.1M views", "category": "Technology"},
    {"id": 5, "title": "Easy Pasta Recipe for Beginners", "channel": "Kitchen Basics", "views": "890K views", "category": "Cooking"},
    {"id": 6, "title": "React vs Vue: Which to Learn", "channel": "CodeWithMe", "views": "430K views", "category": "Education"},
    {"id": 7, "title": "Morning Jazz Playlist", "channel": "ChillHop Radio", "views": "3.3M views", "category": "Music"},
    {"id": 8, "title": "NBA Highlights: Best Plays", "channel": "SportsCenter", "views": "1.7M views", "category": "Sports"},
    {"id": 9, "title": "Unboxing the Latest Smartphone", "channel": "TechBuilds", "views": "670K views", "category": "Technology"},
    {"id": 10, "title": "5 Quick Weeknight Dinners", "channel": "Kitchen Basics", "views": "1.1M views", "category": "Cooking"},
]

def get_categories(videos):
    seen = []
    for v in videos:
        if v["category"] not in seen:
            seen.append(v["category"])
    return seen

def matches(video, query, category):
    title_ok = query.lower() in video["title"].lower()
    category_ok = category is None or video["category"] == category
    return title_ok and category_ok

@component
def VideoGrid():
    query, set_query = use_state("")
    active_category, set_active_category = use_state(None)

    categories = get_categories(VIDEOS)
    filtered = [v for v in VIDEOS if matches(v, query, active_category)]

    def select_category(cat):
        set_active_category(None if cat == "All" else cat)

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
        div(class_name="chip-row",
            button(
                class_name=f"chip {'chip-active' if active_category is None else ''}",
                on_click=lambda: select_category("All"),
                "All",
            ),
            *[
                button(
                    key=cat,
                    class_name=f"chip {'chip-active' if active_category == cat else ''}",
                    on_click=lambda cat=cat: select_category(cat),
                    cat,
                )
                for cat in categories
            ],
        ),
        p(class_name="empty-state", "No videos found.") if len(filtered) == 0 else div(
            class_name="video-grid",
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
