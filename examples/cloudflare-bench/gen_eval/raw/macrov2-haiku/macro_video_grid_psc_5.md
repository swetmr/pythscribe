<!-- exp=macrov2-haiku task=macro_video_grid cond=psc sample=5 model=claude-haiku-4-5-20251001 cost_usd=0.058517200000000005 -->
```python
"use client"
R*

@c
def VideoGrid():
    videos = [
        {"id": 1, "title": "Introduction to Python", "channel": "Tech Academy", "views": 125000, "category": "Education"},
        {"id": 2, "title": "JavaScript Tips and Tricks", "channel": "Code Masters", "views": 89000, "category": "Programming"},
        {"id": 3, "title": "Web Design Tutorial", "channel": "Design Lab", "views": 234000, "category": "Design"},
        {"id": 4, "title": "React for Beginners", "channel": "Code Masters", "views": 456000, "category": "Programming"},
        {"id": 5, "title": "CSS Grid Mastery", "channel": "Design Lab", "views": 178000, "category": "Design"},
        {"id": 6, "title": "Advanced Python Concepts", "channel": "Tech Academy", "views": 267000, "category": "Education"},
        {"id": 7, "title": "UI/UX Design Principles", "channel": "Design Lab", "views": 345000, "category": "Design"},
        {"id": 8, "title": "Machine Learning Basics", "channel": "Tech Academy", "views": 512000, "category": "Education"},
        {"id": 9, "title": "Vue.js Fundamentals", "channel": "Code Masters", "views": 198000, "category": "Programming"},
    ]
    
    categories = ["All", "Programming", "Design", "Education"]
    search, set_search = us("")
    active_category, set_active_category = us("All")
    
    def mk_handler(cat):
        return lambda: set_active_category(cat)
    
    filtered = [v for v in videos
                if search.lower() in v["title"].lower() and
                (active_category == "All" or v["category"] == active_category)]
    
    return (
        style("""
            .video-grid-container { font-family: system-ui, sans-serif; padding: 20px; max-width: 1200px; margin: 0 auto; }
            .search-section { margin-bottom: 20px; }
            .search-input { width: 100%; padding: 10px; font-size: 16px; border: 1px solid #ccc; border-radius: 4px; }
            .category-chips { display: flex; gap: 10px; margin-bottom: 20px; flex-wrap: wrap; }
            .chip { padding: 8px 16px; border: 1px solid #ddd; background: #f5f5f5; border-radius: 20px; cursor: pointer; transition: all 0.2s; }
            .chip.active { background: #3b82f6; color: white; border-color: #3b82f6; }
            .video-grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(250px, 1fr)); gap: 20px; }
            .video-card { border: 1px solid #eee; border-radius: 8px; padding: 15px; transition: box-shadow 0.2s; }
            .video-card:hover { box-shadow: 0 4px 12px rgba(0,0,0,0.1); }
            .video-title { margin: 0 0 10px 0; font-size: 16px; font-weight: 600; }
            .video-channel { margin: 5px 0; font-size: 14px; color: #666; }
            .video-views { margin: 5px 0; font-size: 13px; color: #999; }
            .empty-state { text-align: center; padding: 40px; color: #666; }
        """),
        div(cn="video-grid-container",
            div(cn="search-section",
                input(ph="Search videos...", oh=lambda e: set_search(e.target.value), cn="search-input"),
            ),
            div(cn="category-chips",
                *[button(key=cat, cn=f"chip {('active' if cat == active_category else '')}", oc=mk_handler(cat), cat)
                  for cat in categories]
            ),
            (div(cn="video-grid",
                *[div(key=v["id"], cn="video-card",
                    h3(cn="video-title", v["title"]),
                    p(cn="video-channel", v["channel"]),
                    p(cn="video-views", f"{v['views']:,} views"),
                ) for v in filtered])
             if filtered else
             div(cn="empty-state", p("No videos match your search"))),
        ),
    )
```
