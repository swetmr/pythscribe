<!-- exp=macrov2-haiku task=macro_video_grid cond=ps sample=4 model=claude-haiku-4-5-20251001 cost_usd=0.0534527 -->
```python
"use client"

from pyths.react import component, use_state

@component
def VideoGrid():
    videos = [
        {"id": 1, "title": "Building a React App", "channel": "Code Masters", "views": "124K", "category": "Education"},
        {"id": 2, "title": "JavaScript Tips and Tricks", "channel": "Dev Channel", "views": "89K", "category": "Education"},
        {"id": 3, "title": "Music Video 2024", "channel": "Artist Channel", "views": "2.3M", "category": "Music"},
        {"id": 4, "title": "Gaming Highlights Compilation", "channel": "Gaming Pro", "views": "567K", "category": "Gaming"},
        {"id": 5, "title": "Cooking Pasta Carbonara", "channel": "Chef's Kitchen", "views": "234K", "category": "Cooking"},
        {"id": 6, "title": "Web Design Trends", "channel": "Design Weekly", "views": "156K", "category": "Education"},
        {"id": 7, "title": "Workout Routine for Beginners", "channel": "Fitness First", "views": "445K", "category": "Fitness"},
        {"id": 8, "title": "Travel Vlog: Japan", "channel": "Adventure Channel", "views": "892K", "category": "Travel"},
        {"id": 9, "title": "Machine Learning Basics", "channel": "AI Academy", "views": "321K", "category": "Education"},
        {"id": 10, "title": "Latest Gaming News", "channel": "Gaming Pro", "views": "134K", "category": "Gaming"},
    ]
    
    categories = ["All", "Education", "Music", "Gaming", "Cooking", "Fitness", "Travel"]
    
    search_text, set_search_text = use_state("")
    selected_category, set_selected_category = use_state("All")
    
    filtered_videos = [v for v in videos 
                       if (not search_text or search_text.lower() in v["title"].lower())
                       and (selected_category == "All" or v["category"] == selected_category)]
    
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
                on_click=lambda: set_selected_category(cat),
                class_name="chip active" if selected_category == cat else "chip",
                cat
            ) for cat in categories]
        ),
        
        div(class_name="videos-container",
            *([div(class_name="video-card",
                   key=v["id"],
                   div(class_name="video-title", v["title"]),
                   div(class_name="video-channel", v["channel"]),
                   div(class_name="video-views", f"{v['views']} views")
              ) for v in filtered_videos]
              if filtered_videos
              else [div(class_name="empty-state",
                        p("No videos found. Try a different search or category."))])
        )
    )
```
