<!-- exp=macrov2-haiku task=macro_video_grid cond=ps sample=2 model=claude-haiku-4-5-20251001 cost_usd=0.051042699999999996 -->
```python
"use client"

from pyths.react import component, use_state

VIDEOS = [
    {"id": 1, "title": "Learn Python in 30 Minutes", "channel": "Code Masters", "views": 125000, "category": "Education"},
    {"id": 2, "title": "Amazing Nature Documentary", "channel": "Wildlife Films", "views": 856000, "category": "Nature"},
    {"id": 3, "title": "Gaming Highlights 2024", "channel": "ProGamer TV", "views": 342000, "category": "Gaming"},
    {"id": 4, "title": "Cooking Italian Pasta", "channel": "Chef's Kitchen", "views": 234000, "category": "Cooking"},
    {"id": 5, "title": "Web Development Tutorial", "channel": "Code Masters", "views": 567000, "category": "Education"},
    {"id": 6, "title": "Epic Gaming Marathon", "channel": "ProGamer TV", "views": 789000, "category": "Gaming"},
    {"id": 7, "title": "React Best Practices", "channel": "Dev Academy", "views": 445000, "category": "Education"},
    {"id": 8, "title": "Nature Wildlife Africa", "channel": "Wildlife Films", "views": 1203000, "category": "Nature"},
    {"id": 9, "title": "Baking Chocolate Cake", "channel": "Chef's Kitchen", "views": 189000, "category": "Cooking"},
    {"id": 10, "title": "JavaScript Tips and Tricks", "channel": "Dev Academy", "views": 654000, "category": "Education"},
]

CATEGORIES = ["All", "Education", "Nature", "Gaming", "Cooking"]

def format_views(views):
    if views >= 1000000:
        return f"{views / 1000000:.1f}M"
    elif views >= 1000:
        return f"{views / 1000:.0f}K"
    else:
        return str(views)

@component
def VideoCard(video):
    return div(class_name="video-card",
        div(class_name="video-thumbnail"),
        div(class_name="video-info",
            h3(class_name="video-title", video["title"]),
            p(class_name="video-channel", video["channel"]),
            p(class_name="video-views", f"{format_views(video['views'])} views"),
        ),
    )

@component
def VideoGrid():
    search_text, set_search_text = use_state("")
    selected_category, set_selected_category = use_state("All")
    
    filtered_videos = [v for v in VIDEOS 
                      if search_text.lower() in v["title"].lower() 
                      and (selected_category == "All" or v["category"] == selected_category)]
    
    return div(class_name="video-grid-container",
        div(class_name="search-container",
            input(
                class_name="search-input",
                type="text",
                placeholder="Search videos...",
                value=search_text,
                on_change=lambda e: set_search_text(e.target.value),
            ),
        ),
        
        div(class_name="category-chips",
            *[button(
                key=cat,
                class_name="category-chip" + (" active" if cat == selected_category else ""),
                on_click=lambda: set_selected_category(cat),
                cat,
            ) for cat in CATEGORIES],
        ),
        
        div(class_name="videos-grid",
            *[VideoCard(v, key=v["id"]) for v in filtered_videos],
        ) if filtered_videos else div(class_name="empty-state",
            p("No videos found. Try a different search or category."),
        ),
    )
```
