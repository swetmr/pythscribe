<!-- exp=macrov2-haiku task=macro_video_grid cond=ps sample=3 model=claude-haiku-4-5-20251001 cost_usd=0.0342527 -->
```python
"use client"

from pyths.react import component, use_state

VIDEOS = [
    {"id": 1, "title": "Learn Python in 30 Minutes", "channel": "Code Masters", "views": 125000, "category": "Education"},
    {"id": 2, "title": "5-Minute Workout Routine", "channel": "FitLife", "views": 250000, "category": "Fitness"},
    {"id": 3, "title": "How to Cook Perfect Pasta", "channel": "Cooking with Maria", "views": 89000, "category": "Cooking"},
    {"id": 4, "title": "Galaxy Photography Tips", "channel": "Night Sky", "views": 456000, "category": "Photography"},
    {"id": 5, "title": "React Hooks Explained", "channel": "Tech Academy", "views": 320000, "category": "Education"},
    {"id": 6, "title": "10-Minute Meditation Guide", "channel": "Mindfulness Daily", "views": 178000, "category": "Fitness"},
    {"id": 7, "title": "DIY Home Garden Setup", "channel": "Green Living", "views": 210000, "category": "Gardening"},
    {"id": 8, "title": "Dessert Decorating Techniques", "channel": "Baking Basics", "views": 342000, "category": "Cooking"},
    {"id": 9, "title": "Landscape Photography Essentials", "channel": "Pro Photographers", "views": 501000, "category": "Photography"},
]

CATEGORIES = sorted(list(set(v["category"] for v in VIDEOS)))

def format_views(views):
    if views >= 1000000:
        return f"{views / 1000000:.1f}M"
    elif views >= 1000:
        return f"{views / 1000:.0f}K"
    else:
        return str(views)

@component
def VideoGrid():
    search_text, set_search_text = use_state("")
    selected_category, set_selected_category = use_state(None)
    
    filtered = VIDEOS
    if search_text:
        filtered = [v for v in filtered if search_text.lower() in v["title"].lower()]
    if selected_category:
        filtered = [v for v in filtered if v["category"] == selected_category]
    
    return div(class_name="video-grid-container",
        div(class_name="search-bar",
            input(
                type="text",
                placeholder="Search videos...",
                value=search_text,
                on_change=lambda e: set_search_text(e.target.value),
                class_name="search-input"
            )
        ),
        div(class_name="category-chips",
            button(
                on_click=lambda: set_selected_category(None),
                class_name="chip" + (" active" if selected_category is None else ""),
                "All"
            ),
            *[button(
                key=cat,
                on_click=lambda: set_selected_category(cat),
                class_name="chip" + (" active" if selected_category == cat else ""),
                cat
            ) for cat in CATEGORIES]
        ),
        div(class_name="video-cards-grid",
            *[div(
                key=v["id"],
                class_name="video-card",
                div(class_name="video-title", v["title"]),
                div(class_name="video-channel", v["channel"]),
                div(class_name="video-views", f"{format_views(v['views'])} views")
            ) for v in filtered]
        ) if filtered else div(class_name="empty-state", "No videos match your search.")
    )
```
