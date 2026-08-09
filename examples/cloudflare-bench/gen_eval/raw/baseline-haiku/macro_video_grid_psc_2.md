<!-- exp=baseline-haiku task=macro_video_grid cond=psc sample=2 model=claude-haiku-4-5-20251001 cost_usd=0.025124699999999996 -->
```python
"use client"
R*

@c
def VideoGrid():
    search_text, set_search_text = us("")
    selected_category, set_selected_category = us(None)
    
    videos = [
        {"id": 1, "title": "Learning Python Basics", "channel": "Code Academy", "views": 125000, "category": "Education"},
        {"id": 2, "title": "React Hooks Deep Dive", "channel": "Dev Tutorials", "views": 89500, "category": "Education"},
        {"id": 3, "title": "Gaming Setup Tour 2024", "channel": "Tech Reviews", "views": 340000, "category": "Technology"},
        {"id": 4, "title": "Cooking Italian Pasta", "channel": "Chef Daily", "views": 205000, "category": "Cooking"},
        {"id": 5, "title": "Web Design Trends", "channel": "Design School", "views": 156000, "category": "Design"},
        {"id": 6, "title": "Morning Workout Routine", "channel": "Fitness Pro", "views": 420000, "category": "Fitness"},
        {"id": 7, "title": "Machine Learning Explained", "channel": "AI Institute", "views": 278000, "category": "Education"},
        {"id": 8, "title": "Minimalist Lifestyle Tips", "channel": "Simple Living", "views": 195000, "category": "Lifestyle"},
        {"id": 9, "title": "Budget Gaming PC Build", "channel": "Tech Reviews", "views": 512000, "category": "Technology"},
    ]
    
    categories = sorted(set(v["category"] for v in videos))
    
    filtered = videos
    if search_text:
        filtered = [v for v in filtered if search_text.lower() in v["title"].lower()]
    if selected_category:
        filtered = [v for v in filtered if v["category"] == selected_category]
    
    content = div(cn="video-grid",
        *[div(cn="video-card", key=v["id"],
            div(cn="video-title", v["title"]),
            div(cn="video-channel", v["channel"]),
            div(cn="video-views", f"{v['views']:,} views")
        ) for v in filtered]
    ) if filtered else div(cn="empty-state", "No videos found. Try adjusting your filters.")
    
    return div(cn="video-grid-container",
        div(cn="search-container",
            input(
                type="text",
                cn="search-input",
                ph="Search videos...",
                value=search_text,
                oh=lambda e: set_search_text(e.target.value)
            )
        ),
        div(cn="category-chips",
            button(
                cn="chip" + (" active" if selected_category is None else ""),
                oc=lambda: set_selected_category(None),
                "All"
            ),
            *[button(
                cn="chip" + (" active" if selected_category == cat else ""),
                oc=lambda cat=cat: set_selected_category(cat),
                cat
            ) for cat in categories]
        ),
        content,
    )
```
