<!-- exp=macrov2-sonnet task=macro_video_grid cond=ps sample=4 model=claude-sonnet-5 cost_usd=0.0283824 -->
```python
"use client"

from pyths.react import component, use_state

CATEGORIES = ["All", "Music", "Gaming", "Tech", "Cooking", "Sports", "Education", "Travel"]

VIDEOS = [
    {"id": 1, "title": "Lofi Beats to Study To", "channel": "Chillhop Music", "views": "2.3M views", "category": "Music"},
    {"id": 2, "title": "Speedrunning Every Mario Game", "channel": "GameFast", "views": "890K views", "category": "Gaming"},
    {"id": 3, "title": "Building a Rust Compiler From Scratch", "channel": "DevDive", "views": "154K views", "category": "Tech"},
    {"id": 4, "title": "One Pan Pasta in 15 Minutes", "channel": "Kitchen Basics", "views": "1.1M views", "category": "Cooking"},
    {"id": 5, "title": "Top 10 Goals of the Season", "channel": "Football Weekly", "views": "3.4M views", "category": "Sports"},
    {"id": 6, "title": "Learn Linear Algebra in a Weekend", "channel": "MathClear", "views": "412K views", "category": "Education"},
    {"id": 7, "title": "Backpacking Through Vietnam", "channel": "Wander Free", "views": "670K views", "category": "Travel"},
    {"id": 8, "title": "New Album Reaction", "channel": "Chillhop Music", "views": "230K views", "category": "Music"},
    {"id": 9, "title": "React vs Svelte in 2026", "channel": "DevDive", "views": "98K views", "category": "Tech"},
    {"id": 10, "title": "Mastering the Perfect Omelette", "channel": "Kitchen Basics", "views": "540K views", "category": "Cooking"},
]


def matches_query(video, query):
    return query.lower() in video["title"].lower()


def matches_category(video, category):
    if category == "All":
        return True
    return video["category"] == category


@component
def VideoCard(video):
    return div(class_name="video-card",
        div(class_name="video-thumbnail"),
        div(class_name="video-info",
            h3(class_name="video-title", video["title"]),
            p(class_name="video-channel", video["channel"]),
            p(class_name="video-views", video["views"]),
        ),
    )


@component
def CategoryChip(name, active, on_select):
    chip_class = "chip chip-active" if active else "chip"
    return button(class_name=chip_class, on_click=lambda: on_select(name), name)


@component
def VideoGrid():
    query, set_query = use_state("")
    active_category, set_active_category = use_state("All")

    def select_category(name):
        set_active_category(name)

    filtered = [
        v for v in VIDEOS
        if matches_query(v, query) and matches_category(v, active_category)
    ]

    return div(class_name="video-grid-app",
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
            *[CategoryChip(key=name, name=name, active=name == active_category, on_select=select_category)
              for name in CATEGORIES],
        ),
        div(class_name="video-grid", *[VideoCard(key=v["id"], video=v) for v in filtered])
            if len(filtered) > 0
            else div(class_name="empty-state", p("No videos found.")),
    )
```
