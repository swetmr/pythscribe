<!-- exp=macrov2-sonnet task=macro_video_grid cond=ps sample=5 model=claude-sonnet-5 cost_usd=0.031142399999999997 -->
```python
"use client"

from pyths.react import component, use_state

VIDEOS = [
    {"id": 1, "title": "Learning React Hooks in 20 Minutes", "channel": "CodeWithAda", "views": "1.2M views", "category": "Programming"},
    {"id": 2, "title": "Piano Relaxing Music for Study", "channel": "CalmSounds", "views": "845K views", "category": "Music"},
    {"id": 3, "title": "Top 10 Football Goals This Week", "channel": "SportsCentral", "views": "2.3M views", "category": "Sports"},
    {"id": 4, "title": "Building a REST API with Node.js", "channel": "CodeWithAda", "views": "560K views", "category": "Programming"},
    {"id": 5, "title": "Guitar Cover: Classic Rock Hits", "channel": "StringTheory", "views": "310K views", "category": "Music"},
    {"id": 6, "title": "How Basketball Changed Forever", "channel": "SportsCentral", "views": "980K views", "category": "Sports"},
    {"id": 7, "title": "Documentary: Life in the Deep Ocean", "channel": "NatureLens", "views": "4.1M views", "category": "Documentary"},
    {"id": 8, "title": "Python Tricks Every Developer Should Know", "channel": "CodeWithAda", "views": "1.8M views", "category": "Programming"},
    {"id": 9, "title": "The History of Jazz Explained", "channel": "CalmSounds", "views": "220K views", "category": "Music"},
    {"id": 10, "title": "Documentary: Mountains of the World", "channel": "NatureLens", "views": "1.5M views", "category": "Documentary"},
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
            button(
                class_name=f"chip {'active' if active_category is None else ''}",
                on_click=lambda: select_category(None),
                "All",
            ),
            *[button(
                key=cat,
                class_name=f"chip {'active' if active_category == cat else ''}",
                on_click=lambda cat=cat: select_category(cat) if False else select_category(cat),
                cat,
              ) for cat in categories],
        ),
        p(f"{len(filtered)} results found", class_name="results-count"),
        div(class_name="video-grid", *[
            div(key=v["id"], class_name="video-card",
                div(class_name="video-thumbnail"),
                div(class_name="video-info",
                    h3(class_name="video-title", v["title"]),
                    p(class_name="video-channel", v["channel"]),
                    p(class_name="video-views", v["views"]),
                ),
            ) for v in filtered
        ]) if len(filtered) > 0 else div(class_name="empty-state",
            p("No videos found matching your search."),
        ),
    )
```
