<!-- exp=macrov2-opus task=macro_video_grid cond=ps sample=5 model=claude-opus-4-8 cost_usd=0.041837 -->
```python
"use client"

from pyths.react import component, use_state

VIDEOS = [
    {"id": 1, "title": "Learn PythScribe in 10 Minutes", "channel": "CodeCraft", "views": 120000, "category": "Education"},
    {"id": 2, "title": "Rust Toolchain Deep Dive", "channel": "SystemsGuru", "views": 88000, "category": "Tech"},
    {"id": 3, "title": "Lofi Beats to Compile To", "channel": "ChillWave", "views": 2400000, "category": "Music"},
    {"id": 4, "title": "Building a React Grid from Scratch", "channel": "CodeCraft", "views": 54000, "category": "Tech"},
    {"id": 5, "title": "Top 10 Cooking Hacks", "channel": "KitchenLab", "views": 987000, "category": "Cooking"},
    {"id": 6, "title": "Marathon Training Explained", "channel": "RunFast", "views": 43000, "category": "Sports"},
    {"id": 7, "title": "Understanding Compilers", "channel": "SystemsGuru", "views": 176000, "category": "Education"},
    {"id": 8, "title": "Epic Guitar Solo Compilation", "channel": "ChillWave", "views": 615000, "category": "Music"},
    {"id": 9, "title": "The Science of Sourdough", "channel": "KitchenLab", "views": 302000, "category": "Cooking"},
    {"id": 10, "title": "Basketball Highlights of the Year", "channel": "RunFast", "views": 1120000, "category": "Sports"},
]


def format_views(n):
    if n >= 1000000:
        return f"{n / 1000000:.1f}M views"
    if n >= 1000:
        return f"{n / 1000:.0f}K views"
    return f"{n} views"


def categories_of(videos):
    seen = []
    for v in videos:
        if v["category"] not in seen:
            seen.append(v["category"])
    return ["All", *seen]


@component
def VideoGrid():
    query, set_query = use_state("")
    active_cat, set_active_cat = use_state("All")

    cats = categories_of(VIDEOS)
    q = query.strip().lower()

    visible = [
        v for v in VIDEOS
        if (q == "" or q in v["title"].lower())
        and (active_cat == "All" or v["category"] == active_cat)
    ]

    return div(class_name="video-grid",
        header(class_name="video-grid-header",
            input(
                class_name="video-search",
                type="text",
                value=query,
                placeholder="Search videos...",
                on_change=lambda e: set_query(e.target.value),
            ),
        ),
        div(class_name="chip-row",
            *[button(
                key=c,
                class_name="chip active" if c == active_cat else "chip",
                on_click=lambda: set_active_cat(c),
                c,
            ) for c in cats]
        ),
        div(class_name="video-cards",
            *[div(key=v["id"], class_name="video-card",
                div(class_name="video-thumb"),
                h3(class_name="video-title", v["title"]),
                p(class_name="video-channel", v["channel"]),
                p(class_name="video-views", format_views(v["views"])),
            ) for v in visible]
        ) if visible else div(class_name="empty-state",
            p("No videos match your search."),
        ),
    )
```
