<!-- exp=baseline-001 task=macro_video_grid cond=ps sample=1 model=claude-opus-4-8 cost_usd=0.074548 -->
```python
"use client"

from pyths.react import component, use_state

VIDEOS = [
    {"id": 1, "title": "Build a REST API in Rust", "channel": "Ferrous Dev", "views": 128000, "category": "Rust"},
    {"id": 2, "title": "React Hooks Deep Dive", "channel": "Frontend Weekly", "views": 402500, "category": "React"},
    {"id": 3, "title": "Python Async Explained", "channel": "PyCore", "views": 89300, "category": "Python"},
    {"id": 4, "title": "Understanding Rust Ownership", "channel": "Ferrous Dev", "views": 254100, "category": "Rust"},
    {"id": 5, "title": "State Management in React", "channel": "Frontend Weekly", "views": 176800, "category": "React"},
    {"id": 6, "title": "Data Classes in Python", "channel": "PyCore", "views": 61200, "category": "Python"},
    {"id": 7, "title": "CSS Grid From Scratch", "channel": "Layout Lab", "views": 33400, "category": "CSS"},
    {"id": 8, "title": "Flexbox Mastery", "channel": "Layout Lab", "views": 51900, "category": "CSS"},
    {"id": 9, "title": "Async Rust With Tokio", "channel": "Ferrous Dev", "views": 143700, "category": "Rust"},
    {"id": 10, "title": "React Server Components", "channel": "Frontend Weekly", "views": 298600, "category": "React"},
]

CATEGORIES = ["All", "Rust", "React", "Python", "CSS"]


def format_views(n):
    if n >= 1000000:
        return f"{n / 1000000:.1f}M views"
    if n >= 1000:
        return f"{n / 1000:.1f}K views"
    return f"{n} views"


@component
def VideoGrid():
    query, set_query = use_state("")
    category, set_category = use_state("All")

    q = query.strip().lower()
    matches = [
        v for v in VIDEOS
        if q in v["title"].lower()
        and (category == "All" or v["category"] == category)
    ]

    return div(class_name="video-grid",
        header(class_name="vg-header",
            h1("VideoTube"),
            input(
                class_name="vg-search",
                type="text",
                value=query,
                placeholder="Search videos...",
                on_change=lambda e: set_query(e.target.value),
            ),
        ),
        div(class_name="vg-chips",
            *[button(
                key=c,
                class_name=("vg-chip active" if c == category else "vg-chip"),
                on_click=lambda c=c: set_category(c),
                c,
            ) for c in CATEGORIES],
        ),
        div(class_name="vg-cards",
            *[div(key=v["id"], class_name="vg-card",
                div(class_name="vg-thumb"),
                h3(class_name="vg-title", v["title"]),
                p(class_name="vg-channel", v["channel"]),
                p(class_name="vg-views", format_views(v["views"])),
            ) for v in matches],
        ) if matches else div(class_name="vg-empty",
            p("No videos match your search."),
        ),
    )
```
