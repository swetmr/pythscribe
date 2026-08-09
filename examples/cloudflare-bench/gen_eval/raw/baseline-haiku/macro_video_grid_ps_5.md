<!-- exp=baseline-haiku task=macro_video_grid cond=ps sample=5 model=claude-haiku-4-5-20251001 cost_usd=0.044505499999999996 -->
```python
"use client"

from pyths.react import component, use_state

@component
def VideoGrid():
    # Fixture data
    videos = [
        {"id": 1, "title": "Building Modern Web Apps", "channel": "Dev Tutorial", "views": 152400, "category": "Technology"},
        {"id": 2, "title": "Cooking for Beginners", "channel": "Easy Meals", "views": 89300, "category": "Cooking"},
        {"id": 3, "title": "React Best Practices", "channel": "Code Masters", "views": 234500, "category": "Technology"},
        {"id": 4, "title": "Homemade Pizza Recipes", "channel": "Food Channel", "views": 567200, "category": "Cooking"},
        {"id": 5, "title": "Fitness Training Guide", "channel": "Fit Life", "views": 423100, "category": "Fitness"},
        {"id": 6, "title": "JavaScript Tips and Tricks", "channel": "Dev Tutorial", "views": 178900, "category": "Technology"},
        {"id": 7, "title": "Yoga for Stress Relief", "channel": "Wellness Hub", "views": 312600, "category": "Fitness"},
        {"id": 8, "title": "Gourmet Cooking Techniques", "channel": "Food Channel", "views": 198700, "category": "Cooking"},
        {"id": 9, "title": "Advanced CSS Layouts", "channel": "Code Masters", "views": 289300, "category": "Technology"},
    ]
    
    # State
    search_text, set_search_text = use_state("")
    selected_category, set_selected_category = use_state("All")
    
    # Get unique categories
    categories = ["All"] + sorted(list(set([v["category"] for v in videos])))
    
    # Filter videos
    filtered = videos
    if search_text:
        filtered = [v for v in filtered if search_text.lower() in v["title"].lower()]
    if selected_category != "All":
        filtered = [v for v in filtered if v["category"] == selected_category]
    
    # Format views with K/M suffix
    def format_views(views):
        if views >= 1000000:
            return f"{views / 1000000:.1f}M"
        elif views >= 1000:
            return f"{views / 1000:.1f}K"
        else:
            return str(views)
    
    return div(class_name="video-grid-container",
        div(class_name="search-section",
            input(
                type="text",
                class_name="search-input",
                placeholder="Search videos...",
                value=search_text,
                on_change=lambda e: set_search_text(e.target.value)
            )
        ),
        div(class_name="category-chips",
            *[button(
                key=cat,
                class_name="chip active" if cat == selected_category else "chip",
                on_click=lambda c=cat: set_selected_category(c),
                cat
            ) for cat in categories]
        ),
        div(class_name="videos-grid",
            *([div(class_name="video-card", key=v["id"],
                h3(class_name="video-title", v["title"]),
                p(class_name="video-channel", v["channel"]),
                p(class_name="video-views", f"{format_views(v['views'])} views")
            ) for v in filtered] if filtered else [div(class_name="empty-state", "No videos found. Try a different search or category.")])
        )
    )
```
