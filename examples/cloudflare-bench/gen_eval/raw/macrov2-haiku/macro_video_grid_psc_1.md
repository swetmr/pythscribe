<!-- exp=macrov2-haiku task=macro_video_grid cond=psc sample=1 model=claude-haiku-4-5-20251001 cost_usd=0.0637746 -->
```python
"use client"
R*

videos_data = [
    {"title": "Learn React Hooks", "channel": "Code Masters", "views": 125400, "category": "Tutorial"},
    {"title": "10 CSS Tips & Tricks", "channel": "Web Dev Simplified", "views": 89300, "category": "Tutorial"},
    {"title": "Building a Blog App", "channel": "Code Masters", "views": 234500, "category": "Tutorial"},
    {"title": "Next.js Production Guide", "channel": "Traversy Media", "views": 567890, "category": "Tutorial"},
    {"title": "JavaScript ES2024 Features", "channel": "Web Dev Simplified", "views": 445600, "category": "Tutorial"},
    {"title": "Design System Best Practices", "channel": "Figma Experts", "views": 198300, "category": "Design"},
    {"title": "Accessibility in Modern Web", "channel": "A11y Academy", "views": 76500, "category": "Accessibility"},
    {"title": "TypeScript Advanced Patterns", "channel": "Type Safe Dev", "views": 312400, "category": "Tutorial"},
    {"title": "Responsive Design Masterclass", "channel": "Figma Experts", "views": 289700, "category": "Design"},
]

def get_categories():
    return sorted(set(v["category"] for v in videos_data))

@c
def VideoGrid():
    search_query, set_search_query = us("")
    category_filter, set_category_filter = us(None)
    
    filtered = [v for v in videos_data
                if search_query.lower() in v["title"].lower()
                and (category_filter is None or v["category"] == category_filter)]
    
    categories = get_categories()
    
    return div(cn="video-grid-container",
        div(cn="search-section",
            input(cn="search-input",
                  value=search_query,
                  oh=lambda e: set_search_query(e.target.value),
                  ph="Search videos..."),
        ),
        div(cn="category-chips",
            button(cn=f"chip {'active' if category_filter is None else ''}",
                   oc=lambda: set_category_filter(None),
                   "All"),
            *[button(cn=f"chip {'active' if category_filter == cat else ''}",
                    oc=lambda: set_category_filter(cat),
                    cat)
              for cat in categories],
        ),
        div(cn="videos-grid",
            *[div(cn="video-card", key=v["title"],
                  div(cn="video-title", v["title"]),
                  div(cn="video-channel", v["channel"]),
                  div(cn="video-views", f"{v['views']:,} views"))
              for v in filtered],
        ) if filtered else div(cn="empty-state-message", "No videos found"),
    )
```
