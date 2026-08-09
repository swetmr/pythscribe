<!-- exp=baseline-haiku task=macro_video_grid cond=ps sample=3 model=claude-haiku-4-5-20251001 cost_usd=0.0349405 -->
```python
"use client"

from pyths.react import component, use_state

@component
def VideoGrid():
    videos = [
        {"id": 1, "title": "Python for Beginners", "channel": "Tech Academy", "views": "1.2M", "category": "Tutorial"},
        {"id": 2, "title": "Web Design Trends 2024", "channel": "Design Pro", "views": "856K", "category": "Design"},
        {"id": 3, "title": "Advanced JavaScript Patterns", "channel": "Code Masters", "views": "2.3M", "category": "Programming"},
        {"id": 4, "title": "Cooking with Gordon Ramsay", "channel": "Culinary Arts", "views": "5.1M", "category": "Cooking"},
        {"id": 5, "title": "React Hooks Deep Dive", "channel": "Code Masters", "views": "1.8M", "category": "Programming"},
        {"id": 6, "title": "Interior Design Makeover", "channel": "Home & Style", "views": "3.2M", "category": "Design"},
        {"id": 7, "title": "Python Data Science 101", "channel": "Tech Academy", "views": "2.7M", "category": "Tutorial"},
        {"id": 8, "title": "Quick Pasta Recipes", "channel": "Culinary Arts", "views": "1.5M", "category": "Cooking"},
        {"id": 9, "title": "CSS Grid Mastery", "channel": "Code Masters", "views": "945K", "category": "Programming"},
        {"id": 10, "title": "Color Theory Basics", "channel": "Design Pro", "views": "678K", "category": "Design"},
    ]
    
    search_text, set_search_text = use_state("")
    selected_category, set_selected_category = use_state("All")
    
    categories = sorted(set([v["category"] for v in videos]))
    category_chips = ["All"] + categories
    
    def matches_filter(video):
        title_match = search_text.lower() in video["title"].lower()
        category_match = selected_category == "All" or video["category"] == selected_category
        return title_match and category_match
    
    filtered_videos = [v for v in videos if matches_filter(v)]
    
    return div(class_name="video-grid-container",
        div(class_name="video-grid-header",
            h1("Video Library"),
            input(
                class_name="search-input",
                type="text",
                placeholder="Search videos by title...",
                value=search_text,
                on_change=lambda e: set_search_text(e.target.value)
            ),
        ),
        
        div(class_name="category-chips",
            *[button(
                class_name=f"chip{' chip-active' if cat == selected_category else ''}",
                on_click=lambda c=cat: set_selected_category(c),
                cat
            ) for cat in category_chips]
        ),
        
        (div(class_name="video-grid",
            *[div(class_name="video-card", key=v["id"],
                div(class_name="video-title", v["title"]),
                div(class_name="video-channel", v["channel"]),
                div(class_name="video-views", f"{v['views']} views"),
            ) for v in filtered_videos]
        ) if filtered_videos else div(class_name="empty-state",
            p("No videos found. Try a different search or category.")
        ))
    )
```
