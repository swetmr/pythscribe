<!-- exp=baseline-001 task=macro_video_grid cond=ps sample=2 model=claude-opus-4-8 cost_usd=0.07444049999999999 -->
```python
"use client"

from pyths.react import component, use_state

VIDEOS = [
    {"id": 1, "title": "Building a REST API in Rust", "channel": "CodeForge", "views": 128000, "category": "Programming"},
    {"id": 2, "title": "Lo-Fi Beats to Study To", "channel": "ChillWave", "views": 2400000, "category": "Music"},
    {"id": 3, "title": "10-Minute Full Body Workout", "channel": "FitDaily", "views": 890000, "category": "Fitness"},
    {"id": 4, "title": "The History of the Roman Empire", "channel": "PastForward", "views": 1500000, "category": "Education"},
    {"id": 5, "title": "React Hooks Explained Simply", "channel": "CodeForge", "views": 456000, "category": "Programming"},
    {"id": 6, "title": "Making Sourdough at Home", "channel": "KitchenLab", "views": 312000, "category": "Cooking"},
    {"id": 7, "title": "Ambient Synth Live Set", "channel": "ChillWave", "views": 78000, "category": "Music"},
    {"id": 8, "title": "Beginner Yoga for Flexibility", "channel": "FitDaily", "views": 640000, "category": "Fitness"},
    {"id": 9, "title": "Understanding Quantum Computing", "channel": "PastForward", "views": 995000, "category": "Education"},
    {"id": 10, "title": "One-Pan Pasta Recipe", "channel": "KitchenLab", "views": 210000, "category": "Cooking"},
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
    active_category, set_active_category = use_state("All")

    chips = categories_of(VIDEOS)
    needle = query.strip().lower()

    visible = [
        v for v in VIDEOS
        if (needle == "" or needle in v["title"].lower())
        and (active_category == "All" or v["category"] == active_category)
    ]

    return div(class_name="video-grid-page",
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
            *[
                button(
                    key=c,
                    class_name="vg-chip active" if c == active_category else "vg-chip",
                    on_click=lambda c=c: set_active_category(c),
                    c,
                )
                for c in chips
            ],
        ),
        div(class_name="vg-grid",
            *[
                div(key=v["id"], class_name="vg-card",
                    div(class_name="vg-thumb", v["category"]),
                    h3(class_name="vg-title", v["title"]),
                    p(class_name="vg-channel", v["channel"]),
                    p(class_name="vg-views", format_views(v["views"])),
                )
                for v in visible
            ],
        ) if len(visible) > 0 else div(class_name="vg-empty",
            p("No videos match your search."),
        ),
    )
```
