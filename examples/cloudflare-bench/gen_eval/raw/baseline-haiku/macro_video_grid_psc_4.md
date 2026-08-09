<!-- exp=baseline-haiku task=macro_video_grid cond=psc sample=4 model=claude-haiku-4-5-20251001 cost_usd=0.039469699999999996 -->
```python
"use client"
R*

@c
def VideoGrid():
    search, set_search = us("")
    category, set_category = us("All")
    
    videos = [
        {"id": 1, "title": "Web Development Tutorial", "channel": "Code Academy", "views": 125000, "category": "Education"},
        {"id": 2, "title": "Gaming Highlights 2024", "channel": "Pro Gamer", "views": 450000, "category": "Gaming"},
        {"id": 3, "title": "Cooking with Julia", "channel": "Kitchen Vibes", "views": 89000, "category": "Cooking"},
        {"id": 4, "title": "Travel Vlog Japan", "channel": "Adventure Seekers", "views": 320000, "category": "Travel"},
        {"id": 5, "title": "Python Programming Advanced", "channel": "Code Academy", "views": 210000, "category": "Education"},
        {"id": 6, "title": "Music Production Tips", "channel": "Beat Maker Pro", "views": 175000, "category": "Music"},
        {"id": 7, "title": "Fitness Challenge Week 1", "channel": "Get Fit Now", "views": 98000, "category": "Fitness"},
        {"id": 8, "title": "Esports Tournament Final", "channel": "Pro Gamer", "views": 520000, "category": "Gaming"},
        {"id": 9, "title": "DIY Home Renovation", "channel": "Home Design Hub", "views": 145000, "category": "DIY"},
    ]
    
    categories = ["All", "Education", "Gaming", "Cooking", "Travel", "Music", "Fitness", "DIY"]
    
    filtered = [v for v in videos if search.lower() in v["title"].lower()]
    if category != "All":
        filtered = [v for v in filtered if v["category"] == category]
    
    return div(cn="video-grid-container",
        div(cn="search-section",
            input(type="text", ph="Search videos...", value=search, oh=lambda e: set_search(e.target.value), cn="search-input")
        ),
        div(cn="category-chips",
            *[button(cn=f"chip {'active' if cat == category else ''}", oc=lambda c=cat: set_category(c), cat) 
              for cat in categories]
        ),
        div(cn="videos-grid",
            *[div(cn="video-card",
                div(cn="video-title", v["title"]),
                div(cn="video-channel", v["channel"]),
                div(cn="video-views", f"{v['views']:,} views")
            ) for v in filtered]
        ) if filtered else div(cn="empty-state", p("No videos match your search"))
    )
```
