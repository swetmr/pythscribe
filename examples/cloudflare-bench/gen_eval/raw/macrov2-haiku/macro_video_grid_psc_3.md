<!-- exp=macrov2-haiku task=macro_video_grid cond=psc sample=3 model=claude-haiku-4-5-20251001 cost_usd=0.025962199999999998 -->
```python
"use client"
R*

VIDEOS = [
    {"id": 1, "title": "How to Learn Python", "channel": "Code Academy", "views": 125000, "category": "Education"},
    {"id": 2, "title": "React Hooks Explained", "channel": "Web Dev Tips", "views": 89000, "category": "Education"},
    {"id": 3, "title": "Epic Gaming Moments", "channel": "Gaming Hub", "views": 2100000, "category": "Gaming"},
    {"id": 4, "title": "Lofi Hip Hop Beats", "channel": "Music Vibes", "views": 5600000, "category": "Music"},
    {"id": 5, "title": "Travel Vlog: Tokyo", "channel": "World Explorer", "views": 340000, "category": "Travel"},
    {"id": 6, "title": "Cooking Perfect Pasta", "channel": "Chef's Kitchen", "views": 156000, "category": "Cooking"},
    {"id": 7, "title": "Gaming Highlights 2024", "channel": "Pro Gamer", "views": 1200000, "category": "Gaming"},
    {"id": 8, "title": "Beautiful Sunset Photography", "channel": "Nature Lens", "views": 450000, "category": "Photography"},
    {"id": 9, "title": "Music Production Tutorial", "channel": "Beat Maker", "views": 230000, "category": "Music"},
]

@c
def VideoGrid():
    search_text, set_search_text = us("")
    category, set_category = us(None)
    
    categories = ["All"] + sorted(set(v["category"] for v in VIDEOS))
    filtered = [v for v in VIDEOS 
                if search_text.lower() in v["title"].lower()
                and (category is None or v["category"] == category)]
    
    return div(cn="video-grid-container",
        div(cn="search-section",
            input(cn="search-input", ph="Search videos...", value=search_text, 
                  oh=lambda e: set_search_text(e.target.value)),
        ),
        div(cn="categories-section",
            *[button(cn=f"category-chip{' active' if (cat == 'All' and category is None) or cat == category else ''}", 
                     oc=lambda c=cat: set_category(None if c == "All" else c),
                     cat)
              for cat in categories],
        ),
        div(cn="video-grid",
            *[div(cn="video-card", key=v["id"],
                  div(cn="video-title", v["title"]),
                  div(cn="video-channel", v["channel"]),
                  div(cn="video-views", f"{v['views']:,} views"),
              )
              for v in filtered]
        ) if filtered else div(cn="empty-state", "No videos found. Try adjusting your search or filters."),
    )
```
