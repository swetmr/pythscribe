<!-- exp=baseline-haiku task=macro_video_grid cond=psc sample=5 model=claude-haiku-4-5-20251001 cost_usd=0.0308047 -->
```python
"use client"
R*

@c
def VideoGrid():
    videos = [
        {"id": 1, "title": "Python Tips and Tricks", "channel": "Tech Daily", "views": 125000, "category": "Programming"},
        {"id": 2, "title": "Web Development Masterclass", "channel": "Code Academy", "views": 89000, "category": "Web"},
        {"id": 3, "title": "Machine Learning 101", "channel": "AI Basics", "views": 203000, "category": "Programming"},
        {"id": 4, "title": "Design Patterns Explained", "channel": "Dev Insights", "views": 67000, "category": "Programming"},
        {"id": 5, "title": "React Hooks Deep Dive", "channel": "React Masters", "views": 145000, "category": "Web"},
        {"id": 6, "title": "CSS Grid Layout", "channel": "Design Lab", "views": 98000, "category": "Web"},
        {"id": 7, "title": "Productivity Tips for Developers", "channel": "Dev Tools", "views": 56000, "category": "Productivity"},
        {"id": 8, "title": "JavaScript ES2024 Features", "channel": "JS Weekly", "views": 178000, "category": "Programming"},
        {"id": 9, "title": "UX Design Principles", "channel": "Design Thinking", "views": 112000, "category": "Design"},
    ]
    
    categories = ["All", "Programming", "Web", "Design", "Productivity"]
    
    search_text, set_search_text = us("")
    selected_category, set_selected_category = us("All")
    
    filtered = [v for v in videos 
                if search_text.lower() in v["title"].lower() and
                   (selected_category == "All" or v["category"] == selected_category)]
    
    def format_views(count):
        if count >= 1000000:
            return f"{count / 1000000:.1f}M"
        elif count >= 1000:
            return f"{count / 1000:.0f}K"
        return str(count)
    
    return div(cn="video-grid-container",
        div(cn="search-section",
            input(type="text", ph="Search videos...", value=search_text, oh=lambda e: set_search_text(e.target.value), cn="search-input")
        ),
        div(cn="category-chips",
            *[button(cn=f"chip {'active' if cat == selected_category else ''}", oc=lambda c=cat: set_selected_category(c), cat)
              for cat in categories]
        ),
        div(cn="video-grid",
            *([div(cn="video-card", key=v["id"],
                h3(cn="video-title", v["title"]),
                p(cn="video-channel", v["channel"]),
                p(cn="video-views", f"{format_views(v['views'])} views")
            ) for v in filtered] if filtered else [p(cn="empty-state", "No videos found. Try a different search or category.")])
        )
    )
```
