<!-- exp=macrov2-haiku task=macro_video_grid cond=ps sample=1 model=claude-haiku-4-5-20251001 cost_usd=0.027094599999999996 -->
```python
"use client"

from pyths.react import component, use_state

@component
def VideoGrid():
    videos = [
        {"id": 1, "title": "Python Tips and Tricks", "channel": "Tech Academy", "views": "245K", "category": "Education"},
        {"id": 2, "title": "Web Development Tutorial", "channel": "Code Masters", "views": "1.2M", "category": "Education"},
        {"id": 3, "title": "Gaming Highlights", "channel": "Pro Gamer", "views": "890K", "category": "Gaming"},
        {"id": 4, "title": "Music Production Basics", "channel": "Beat Maker", "views": "156K", "category": "Music"},
        {"id": 5, "title": "JavaScript Advanced Concepts", "channel": "JS Ninja", "views": "432K", "category": "Education"},
        {"id": 6, "title": "Fortnite Montage", "channel": "Stream King", "views": "2.1M", "category": "Gaming"},
        {"id": 7, "title": "Electronic Music Compilation", "channel": "EDM Channel", "views": "567K", "category": "Music"},
        {"id": 8, "title": "React Hooks Explained", "channel": "Web Dev Pro", "views": "789K", "category": "Education"},
    ]
    
    categories = ["All", "Education", "Gaming", "Music"]
    
    search, set_search = use_state("")
    selected_category, set_selected_category = use_state("All")
    
    def filter_videos():
        filtered = videos
        if search:
            filtered = [v for v in filtered if search.lower() in v["title"].lower()]
        if selected_category != "All":
            filtered = [v for v in filtered if v["category"] == selected_category]
        return filtered
    
    filtered_videos = filter_videos()
    
    return div(class_name="video-grid-container",
        div(class_name="search-section",
            input(
                class_name="search-input",
                type="text",
                placeholder="Search videos...",
                value=search,
                on_change=lambda e: set_search(e.target.value)
            )
        ),
        div(class_name="category-chips",
            *[button(
                key=cat,
                class_name=f"category-chip {'active' if selected_category == cat else ''}",
                on_click=lambda: set_selected_category(cat),
                cat
            ) for cat in categories]
        ),
        (div(class_name="video-grid",
            *[div(class_name="video-card", key=v["id"],
                h3(class_name="video-title", v["title"]),
                p(class_name="video-channel", v["channel"]),
                p(class_name="video-views", f"{v['views']} views")
            ) for v in filtered_videos]
        ) if filtered_videos else div(class_name="empty-state",
            p("No videos found. Try adjusting your search or filters.")
        ))
    )
```
