<!-- exp=macrov2-haiku task=macro_video_grid cond=psc sample=2 model=claude-haiku-4-5-20251001 cost_usd=0.0589722 -->
```python
"use client"
R*

@c
def VideoGrid():
    videos = [
        {"id": 1, "title": "Learn React Hooks", "channel": "Web Dev Academy", "views": 125000, "category": "Education"},
        {"id": 2, "title": "Cooking Pasta", "channel": "Chef's Kitchen", "views": 89000, "category": "Food"},
        {"id": 3, "title": "Travel Vlog: Tokyo", "channel": "Wanderlust Chronicles", "views": 234000, "category": "Travel"},
        {"id": 4, "title": "Python Tutorial", "channel": "Code Masters", "views": 456000, "category": "Education"},
        {"id": 5, "title": "Gaming Highlights", "channel": "Pro Gamer", "views": 312000, "category": "Gaming"},
        {"id": 6, "title": "DIY Home Decor", "channel": "Home & Style", "views": 178000, "category": "Lifestyle"},
        {"id": 7, "title": "Music Production", "channel": "Beat Lab", "views": 95000, "category": "Music"},
        {"id": 8, "title": "Fitness Routine", "channel": "Fit Life", "views": 267000, "category": "Fitness"},
        {"id": 9, "title": "Photography Tips", "channel": "Camera Craft", "views": 142000, "category": "Art"},
    ]
    
    categories = ["All", "Education", "Food", "Travel", "Gaming", "Lifestyle", "Music", "Fitness", "Art"]
    
    search_text, set_search_text = us("")
    active_category, set_active_category = us("All")
    
    def make_handler(c):
        return lambda: set_active_category(c)
    
    filtered = [v for v in videos 
                if search_text.lower() in v["title"].lower()
                and (active_category == "All" or v["category"] == active_category)]
    
    return div(cn="video-grid-container",
        div(cn="header",
            h1("Video Gallery"),
            input(cn="search-input", 
                  value=search_text, 
                  oh=lambda e: set_search_text(e.target.value),
                  ph="Search videos..."),
        ),
        div(cn="category-chips",
            *[button(cn=f"chip{' active' if cat == active_category else ''}",
                     oc=make_handler(cat),
                     cat)
              for cat in categories],
        ),
        div(cn="videos-grid" if filtered else "empty-state",
            *([div(cn="video-card", key=v["id"],
                   div(cn="video-thumbnail", span("▶")),
                   div(cn="video-info",
                       h3(v["title"]),
                       p(cn="channel", v["channel"]),
                       p(cn="views", f"{v['views']:,} views"),
                   ),
               ) for v in filtered] 
              if filtered 
              else [p("No videos found. Try adjusting your search or filters.")])
        ),
    )
```
