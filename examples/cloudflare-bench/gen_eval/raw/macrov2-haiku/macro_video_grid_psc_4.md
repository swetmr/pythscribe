<!-- exp=macrov2-haiku task=macro_video_grid cond=psc sample=4 model=claude-haiku-4-5-20251001 cost_usd=0.0284322 -->
```python
"use client"
R*

@c
def VideoGrid():
    videos = [
        {"title": "Advanced Python Patterns", "channel": "Code Masters", "views": "245K", "category": "Programming"},
        {"title": "Web Design Trends 2024", "channel": "Design Hub", "views": "512K", "category": "Design"},
        {"title": "Machine Learning Basics", "channel": "AI Academy", "views": "128K", "category": "Programming"},
        {"title": "CSS Grid Tutorial", "channel": "Web Dev Pro", "views": "89K", "category": "Design"},
        {"title": "Database Optimization", "channel": "Code Masters", "views": "156K", "category": "Programming"},
        {"title": "UI/UX Best Practices", "channel": "Design Hub", "views": "342K", "category": "Design"},
        {"title": "JavaScript Performance", "channel": "Web Dev Pro", "views": "201K", "category": "Programming"},
        {"title": "Color Theory Masterclass", "channel": "Design Hub", "views": "478K", "category": "Design"},
        {"title": "React Hooks Deep Dive", "channel": "Code Masters", "views": "334K", "category": "Programming"},
        {"title": "Animation Principles", "channel": "Design Hub", "views": "267K", "category": "Design"},
    ]
    
    categories = ["All", "Programming", "Design"]
    
    search_text, set_search_text = us("")
    selected_category, set_selected_category = us("All")
    
    filtered = [v for v in videos
                if search_text.lower() in v["title"].lower()
                and (selected_category == "All" or v["category"] == selected_category)]
    
    content = (div(cn="videos-grid",
        *[div(cn="video-card",
            h3(cn="video-title", v["title"]),
            p(cn="video-channel", v["channel"]),
            p(cn="video-views", f"{v['views']} views")
        ) for v in filtered]
    ) if filtered else div(cn="empty-state",
        p("No videos found. Try adjusting your search or category filter.")
    ))
    
    return div(cn="video-grid-container",
        div(cn="search-section",
            input(
                cn="search-input",
                type="text",
                ph="Search videos...",
                value=search_text,
                oh=lambda e: set_search_text(e.target.value)
            )
        ),
        div(cn="categories-section",
            *[button(
                cn=f"category-chip{' active' if cat == selected_category else ''}",
                oc=lambda c=cat: set_selected_category(c),
                cat
            ) for cat in categories]
        ),
        content
    )
```
