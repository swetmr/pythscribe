<!-- exp=macrov2-sonnet task=macro_video_grid cond=ps sample=1 model=claude-sonnet-5 cost_usd=0.0769386 -->
```python
"use client"

from pyths.react import component, use_state

VIDEOS = [
    {"title": "Learn PythScribe in 10 Minutes", "channel": "CodeCrafters", "views": 152340, "category": "Tech"},
    {"title": "Epic Boss Fight Compilation", "channel": "GameZone", "views": 984210, "category": "Gaming"},
    {"title": "Lo-fi Beats to Study To", "channel": "ChillHop", "views": 2450000, "category": "Music"},
    {"title": "5-Minute Pasta Recipe", "channel": "QuickEats", "views": 88210, "category": "Cooking"},
    {"title": "React Hooks Explained", "channel": "CodeCrafters", "views": 310500, "category": "Tech"},
    {"title": "Speedrun World Record!", "channel": "GameZone", "views": 421900, "category": "Gaming"},
    {"title": "Top 10 Guitar Riffs", "channel": "MusicVibes", "views": 675000, "category": "Music"},
    {"title": "Baking Sourdough Bread", "channel": "QuickEats", "views": 132400, "category": "Cooking"},
    {"title": "Building a Rust Compiler", "channel": "CodeCrafters", "views": 59800, "category": "Tech"},
    {"title": "Retro Console Unboxing", "channel": "GameZone", "views": 210300, "category": "Gaming"},
]


def format_views(v):
    return f"{v:,} views"


@component
def VideoGrid():
    query, set_query = use_state("")
    active_category, set_active_category = use_state(None)

    categories = sorted(set([v["category"] for v in VIDEOS]))

    def matches(v):
        title_match = query.lower() in v["title"].lower()
        category_match = active_category is None or v["category"] == active_category
        return title_match and category_match

    filtered = [v for v in VIDEOS if matches(v)]

    def select_category(cat):
        set_active_category(None if cat == active_category else cat)

    return div(class_name="video-grid-page",
        input(class_name="search-input", value=query,
              on_change=lambda e: set_query(e.target.value),
              placeholder="Search videos..."),
        div(class_name="chip-row",
            button(class_name="chip active" if active_category is None else "chip",
                   on_click=lambda: set_active_category(None), "All"),
            *[button(key=cat,
                     class_name="chip active" if cat == active_category else "chip",
                     on_click=lambda: select_category(cat), cat)
              for cat in categories],
        ),
        p(class_name="empty-state", "No videos found.") if len(filtered) == 0 else div(class_name="video-grid",
            *[div(key=v["title"], class_name="video-card",
                  div(class_name="video-thumb"),
                  h3(class_name="video-title", v["title"]),
                  p(class_name="video-channel", v["channel"]),
                  p(class_name="video-views", format_views(v["views"])),
                 )
              for v in filtered],
        ),
    )
```
