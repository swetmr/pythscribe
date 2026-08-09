<!-- exp=baseline-haiku task=macro_video_grid cond=ps sample=1 model=claude-haiku-4-5-20251001 cost_usd=0.027639300000000002 -->
```python
"use client"

from pyths.react import component, use_state

@component
def VideoGrid():
    videos = [
        {"id": 1, "title": "10 Python Tips Everyone Should Know", "channel": "Tech Hub", "views": 245000, "category": "Education"},
        {"id": 2, "title": "Cooking the Perfect Steak", "channel": "Chef's Kitchen", "views": 523000, "category": "Cooking"},
        {"id": 3, "title": "Web Development Tutorial 2024", "channel": "Code Masters", "views": 789000, "category": "Education"},
        {"id": 4, "title": "Amazing Nature Documentary", "channel": "Nature World", "views": 1200000, "category": "Nature"},
        {"id": 5, "title": "Gaming Highlights Compilation", "channel": "Pro Gamers", "views": 654000, "category": "Gaming"},
        {"id": 6, "title": "Vegan Breakfast Ideas", "channel": "Chef's Kitchen", "views": 342000, "category": "Cooking"},
        {"id": 7, "title": "Machine Learning Explained", "channel": "Tech Hub", "views": 456000, "category": "Education"},
        {"id": 8, "title": "Underwater Exploration", "channel": "Nature World", "views": 987000, "category": "Nature"},
        {"id": 9, "title": "Latest Gaming Review", "channel": "Pro Gamers", "views": 567000, "category": "Gaming"},
    ]
    
    categories = ["All", "Education", "Cooking", "Nature", "Gaming"]
    
    search_text, set_search_text = use_state("")
    selected_category, set_selected_category = use_state("All")
    
    filtered = videos
    if search_text:
        filtered = [v for v in filtered if search_text.lower() in v["title"].lower()]
    if selected_category != "All":
        filtered = [v for v in filtered if v["category"] == selected_category]
    
    return div(class_name="video-grid-container",
        div(class_name="search-section",
            input(
                type="text",
                placeholder="Search videos...",
                value=search_text,
                on_change=lambda e: set_search_text(e.target.value),
                class_name="search-input"
            )
        ),
        div(class_name="category-chips",
            *[button(
                key=cat,
                class_name=f"chip {'active' if cat == selected_category else ''}",
                on_click=lambda c=cat: set_selected_category(c),
                cat
            ) for cat in categories]
        ),
        filtered and div(class_name="grid",
            *[div(class_name="video-card", key=v["id"],
                h3(class_name="video-title", v["title"]),
                p(class_name="video-channel", v["channel"]),
                p(class_name="video-views", f"{v['views']:,} views")
            ) for v in filtered]
        ) or div(class_name="empty-state",
            p("No videos found. Try adjusting your search or filters.")
        )
    )
```
