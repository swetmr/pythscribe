<!-- exp=macrov2-haiku task=macro_video_grid cond=ps sample=5 model=claude-haiku-4-5-20251001 cost_usd=0.028687699999999997 -->
```python
"use client"

from pyths.react import component, use_state

VIDEOS = [
    {"id": 1, "title": "Building a React Dashboard", "channel": "Dev Tips", "views": 125000, "category": "Tutorial"},
    {"id": 2, "title": "Advanced Python Patterns", "channel": "Code Masters", "views": 89000, "category": "Tutorial"},
    {"id": 3, "title": "Gaming Setup Tour 2024", "channel": "Tech Reviews", "views": 450000, "category": "Gaming"},
    {"id": 4, "title": "Cooking Pasta Carbonara", "channel": "Easy Recipes", "views": 230000, "category": "Cooking"},
    {"id": 5, "title": "JavaScript Performance Tips", "channel": "Dev Tips", "views": 156000, "category": "Tutorial"},
    {"id": 6, "title": "DIY PC Build Guide", "channel": "Tech Reviews", "views": 320000, "category": "Gaming"},
    {"id": 7, "title": "Healthy Smoothie Recipes", "channel": "Easy Recipes", "views": 98000, "category": "Cooking"},
    {"id": 8, "title": "Web Design Best Practices", "channel": "Code Masters", "views": 210000, "category": "Design"},
    {"id": 9, "title": "Retro Gaming Collection", "channel": "Tech Reviews", "views": 540000, "category": "Gaming"},
]

CATEGORIES = ["All", "Tutorial", "Gaming", "Cooking", "Design"]

@component
def VideoGrid():
    search_text, set_search_text = use_state("")
    selected_category, set_selected_category = use_state("All")
    
    def matches_search(video):
        return search_text.lower() in video["title"].lower()
    
    def matches_category(video):
        if selected_category == "All":
            return True
        return video["category"] == selected_category
    
    filtered_videos = [v for v in VIDEOS if matches_search(v) and matches_category(v)]
    
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
                on_click=lambda cat=cat: set_selected_category(cat),
                class_name=f"chip {'active' if selected_category == cat else ''}",
                cat
            ) for cat in CATEGORIES]
        ),
        
        div(class_name="videos-grid",
            *[div(class_name="video-card", key=v["id"],
                div(class_name="card-title", v["title"]),
                div(class_name="card-meta",
                    div(class_name="card-channel", v["channel"]),
                    div(class_name="card-views", f"{v['views']:,} views")
                )
            ) for v in filtered_videos]
        ) if filtered_videos else div(class_name="empty-state", "No videos found. Try adjusting your search or filters.")
    )
```
