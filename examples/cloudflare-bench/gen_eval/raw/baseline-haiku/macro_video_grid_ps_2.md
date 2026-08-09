<!-- exp=baseline-haiku task=macro_video_grid cond=ps sample=2 model=claude-haiku-4-5-20251001 cost_usd=0.0462455 -->
```python
"use client"
from pyths.react import component, use_state

@component
def VideoGrid():
    videos = [
        {"id": 1, "title": "Building React Components", "channel": "Web Dev Academy", "views": "245K", "category": "Tutorial"},
        {"id": 2, "title": "Python for Beginners", "channel": "Code Mastery", "views": "892K", "category": "Tutorial"},
        {"id": 3, "title": "Coffee Shop Ambience", "channel": "Relaxing Vibes", "views": "1.2M", "category": "Music"},
        {"id": 4, "title": "Gaming Highlights 2024", "channel": "Pro Gamer", "views": "456K", "category": "Gaming"},
        {"id": 5, "title": "Machine Learning Basics", "channel": "AI Institute", "views": "523K", "category": "Tutorial"},
        {"id": 6, "title": "Cooking Italian Pasta", "channel": "Chef's Kitchen", "views": "678K", "category": "Cooking"},
        {"id": 7, "title": "DIY Home Renovation", "channel": "Home & Style", "views": "345K", "category": "DIY"},
        {"id": 8, "title": "Jazz Music Compilation", "channel": "Relaxing Vibes", "views": "567K", "category": "Music"},
        {"id": 9, "title": "Advanced JavaScript Tips", "channel": "Web Dev Academy", "views": "234K", "category": "Tutorial"},
    ]
    
    categories = ["All", "Tutorial", "Music", "Gaming", "Cooking", "DIY"]
    
    search_text, set_search_text = use_state("")
    selected_category, set_selected_category = use_state("All")
    
    def filter_videos():
        filtered = videos
        
        if search_text:
            search_lower = search_text.lower()
            filtered = [v for v in filtered if search_lower in v["title"].lower()]
        
        if selected_category != "All":
            filtered = [v for v in filtered if v["category"] == selected_category]
        
        return filtered
    
    filtered_videos = filter_videos()
    
    return div(class_name="video-grid-container",
        div(class_name="video-grid-header",
            h1("Video Feed"),
            input(
                type="text",
                class_name="video-grid-search",
                placeholder="Search videos...",
                value=search_text,
                on_change=lambda e: set_search_text(e.target.value)
            )
        ),
        
        div(class_name="category-chips",
            *[button(
                key=category,
                class_name="category-chip" + (" active" if category == selected_category else ""),
                on_click=lambda c=category: set_selected_category(c),
                category
            ) for category in categories]
        ),
        
        div(class_name="video-grid-content",
            (div(class_name="video-grid",
                *[div(key=v["id"], class_name="video-card",
                    div(class_name="video-thumbnail"),
                    div(class_name="video-info",
                        h3(class_name="video-title", v["title"]),
                        p(class_name="video-channel", v["channel"]),
                        p(class_name="video-views", f"{v['views']} views")
                    )
                ) for v in filtered_videos]
            ) if filtered_videos else div(class_name="empty-state",
                p("No videos found. Try adjusting your filters!")
            ))
        )
    )
```
