<!-- exp=baseline-haiku task=macro_video_grid cond=ps sample=4 model=claude-haiku-4-5-20251001 cost_usd=0.0379255 -->
```python
"use client"

from pyths.react import component, use_state

@component
def VideoGrid():
    videos = [
        {"id": 1, "title": "Introduction to Python", "channel": "Tech Academy", "views": 125000, "category": "Programming"},
        {"id": 2, "title": "Web Design Fundamentals", "channel": "Design School", "views": 89000, "category": "Design"},
        {"id": 3, "title": "Advanced JavaScript Tips", "channel": "Code Masters", "views": 234000, "category": "Programming"},
        {"id": 4, "title": "UI/UX Best Practices", "channel": "Design School", "views": 156000, "category": "Design"},
        {"id": 5, "title": "React Hooks Deep Dive", "channel": "Code Masters", "views": 198000, "category": "Programming"},
        {"id": 6, "title": "Building a Blog with Next.js", "channel": "Tech Academy", "views": 145000, "category": "Tutorial"},
        {"id": 7, "title": "CSS Grid Mastery", "channel": "Design School", "views": 112000, "category": "Design"},
        {"id": 8, "title": "Machine Learning Basics", "channel": "AI Institute", "views": 267000, "category": "AI"},
        {"id": 9, "title": "Node.js Performance Optimization", "channel": "Code Masters", "views": 187000, "category": "Programming"},
        {"id": 10, "title": "Data Science with Python", "channel": "AI Institute", "views": 203000, "category": "AI"},
    ]
    
    categories = ["All", "Programming", "Design", "Tutorial", "AI"]
    search_query, set_search_query = use_state("")
    selected_category, set_selected_category = use_state("All")
    
    filtered_videos = videos
    if search_query:
        filtered_videos = [v for v in filtered_videos if search_query.lower() in v["title"].lower()]
    if selected_category != "All":
        filtered_videos = [v for v in filtered_videos if v["category"] == selected_category]
    
    def format_views(views):
        if views >= 1000000:
            return f"{views / 1000000:.1f}M"
        elif views >= 1000:
            return f"{views / 1000:.0f}K"
        return str(views)
    
    return div(class_name="video-grid-container",
        div(class_name="search-section",
            input(
                type="text",
                placeholder="Search videos...",
                value=search_query,
                on_change=lambda e: set_search_query(e.target.value),
                class_name="search-input"
            )
        ),
        div(class_name="category-chips",
            *[button(
                key=cat,
                class_name="chip" + (" active" if selected_category == cat else ""),
                on_click=lambda c=cat: set_selected_category(c),
                cat
            ) for cat in categories]
        ),
        div(class_name="videos-grid",
            *[div(key=v["id"], class_name="video-card",
                h3(v["title"]),
                p(v["channel"]),
                p(f"{format_views(v['views'])} views")
            ) for v in filtered_videos]
        ) if filtered_videos else div(class_name="empty-state",
            p("No videos found. Try a different search or category.")
        )
    )
```
