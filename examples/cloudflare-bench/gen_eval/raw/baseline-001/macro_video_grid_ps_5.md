<!-- exp=baseline-001 task=macro_video_grid cond=ps sample=5 model=claude-opus-4-8 cost_usd=0.043649999999999994 -->
```python
"use client"

from pyths.react import component, use_state

VIDEOS = [
    {"id": 1, "title": "Learn PythScribe in 100 Seconds", "channel": "CodeSpark", "views": 1200000, "category": "Education"},
    {"id": 2, "title": "Lofi Beats to Code To", "channel": "ChillWave", "views": 8400000, "category": "Music"},
    {"id": 3, "title": "Rust for Absolute Beginners", "channel": "CrabTalk", "views": 540000, "category": "Education"},
    {"id": 4, "title": "Speedrunning the Kitchen: 5 Min Pasta", "channel": "QuickBites", "views": 2300000, "category": "Cooking"},
    {"id": 5, "title": "Epic Boss Fight — No Damage Run", "channel": "PixelRush", "views": 970000, "category": "Gaming"},
    {"id": 6, "title": "Morning Yoga Flow for Focus", "channel": "CalmCore", "views": 415000, "category": "Fitness"},
    {"id": 7, "title": "Synthwave Live Mix 2026", "channel": "ChillWave", "views": 3100000, "category": "Music"},
    {"id": 8, "title": "Building a Compiler in a Weekend", "channel": "CodeSpark", "views": 260000, "category": "Education"},
    {"id": 9, "title": "Street Food Tour: Bangkok Night Market", "channel": "QuickBites", "views": 5600000, "category": "Cooking"},
    {"id": 10, "title": "Ranked Grind: Road to Champion", "channel": "PixelRush", "views": 720000, "category": "Gaming"},
]

CATEGORIES = ["All", "Education", "Music", "Cooking", "Gaming", "Fitness"]


def format_views(n):
    if n >= 1000000:
        return f"{n / 1000000:.1f}M views"
    if n >= 1000:
        return f"{n / 1000:.0f}K views"
    return f"{n} views"


@component
def VideoGrid():
    query, set_query = use_state("")
    category, set_category = use_state("All")

    needle = query.strip().lower()
    visible = [
        v for v in VIDEOS
        if (needle == "" or needle in v["title"].lower())
        and (category == "All" or v["category"] == category)
    ]

    return div(class_name="video-grid-page",
        header(class_name="vg-header",
            h1("Home"),
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
                class_name="vg-chip vg-chip-active" if c == category else "vg-chip",
                on_click=lambda c=c: set_category(c),
                c,
            ) for c in CATEGORIES],
        ),
        div(class_name="vg-grid",
            *[div(key=v["id"], class_name="vg-card",
                div(class_name="vg-thumb"),
                h3(class_name="vg-title", v["title"]),
                p(class_name="vg-channel", v["channel"]),
                p(class_name="vg-views", format_views(v["views"])),
            ) for v in visible],
        ) if len(visible) > 0 else div(class_name="vg-empty",
            p("No videos match your search."),
        ),
    )
```
