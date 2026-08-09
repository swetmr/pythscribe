<!-- exp=baseline-haiku task=macro_video_grid cond=psc sample=3 model=claude-haiku-4-5-20251001 cost_usd=0.0322347 -->
```python
"use client"
R*

@c
def VideoGrid():
    videos = [
        {"id": 1, "title": "Learn PythScribe Basics", "channel": "Tech Academy", "views": 15200, "category": "Tutorial"},
        {"id": 2, "title": "Web Development Tips", "channel": "Code Masters", "views": 8900, "category": "Tutorial"},
        {"id": 3, "title": "JavaScript Performance", "channel": "Dev Channel", "views": 25000, "category": "Performance"},
        {"id": 4, "title": "React Best Practices", "channel": "Frontend Pro", "views": 12300, "category": "Tutorial"},
        {"id": 5, "title": "CSS Grid Mastery", "channel": "Design Lab", "views": 9800, "category": "Design"},
        {"id": 6, "title": "State Management Guide", "channel": "Code Masters", "views": 18700, "category": "Tutorial"},
        {"id": 7, "title": "Debugging Strategies", "channel": "Dev Channel", "views": 11200, "category": "Performance"},
        {"id": 8, "title": "Component Architecture", "channel": "Frontend Pro", "views": 14500, "category": "Tutorial"},
        {"id": 9, "title": "Responsive Design Patterns", "channel": "Design Lab", "views": 16400, "category": "Design"},
    ]
    
    categories = ["All", "Tutorial", "Design", "Performance"]
    search, set_search = us("")
    selected_category, set_selected_category = us("All")
    
    filtered = [v for v in videos 
                if search.lower() in v["title"].lower() 
                and (selected_category == "All" or v["category"] == selected_category)]
    
    return div(cn="video-grid-container",
        div(cn="search-section",
            input(ph="Search videos...", value=search, 
                  oh=lambda e: set_search(e.target.value), cn="search-input"),
        ),
        div(cn="category-chips",
            *[button(
                cn=f"chip{' active' if cat == selected_category else ''}", 
                oc=lambda c=cat: set_selected_category(c),
                cat
            ) for cat in categories],
        ),
        (div(cn="video-grid",
            *[div(cn="video-card", key=v["id"],
                h3(cn="video-title", v["title"]),
                p(cn="video-channel", v["channel"]),
                p(cn="video-views", f"{v['views']:,} views"),
            ) for v in filtered],
        ) if filtered else div(cn="empty-state-container",
            p(cn="empty-message", "No videos match your search.")
        )),
    )
```
