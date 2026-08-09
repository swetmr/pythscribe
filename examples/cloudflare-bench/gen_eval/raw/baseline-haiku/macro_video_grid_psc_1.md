<!-- exp=baseline-haiku task=macro_video_grid cond=psc sample=1 model=claude-haiku-4-5-20251001 cost_usd=0.0334433 -->
```python
"use client"
R*

videos = [
    {"title": "Learning Python Basics", "channel": "CodeMasters", "views": 245000, "category": "Programming"},
    {"title": "Web Design Tips", "channel": "Design Daily", "views": 189000, "category": "Design"},
    {"title": "React for Beginners", "channel": "CodeMasters", "views": 523000, "category": "Programming"},
    {"title": "CSS Animations Guide", "channel": "Web Dev Pro", "views": 412000, "category": "Design"},
    {"title": "JavaScript Advanced Topics", "channel": "CodeMasters", "views": 334000, "category": "Programming"},
    {"title": "UX Best Practices", "channel": "Design Daily", "views": 267000, "category": "Design"},
    {"title": "TypeScript Deep Dive", "channel": "Web Dev Pro", "views": 456000, "category": "Programming"},
    {"title": "Mobile App Design", "channel": "Design Daily", "views": 198000, "category": "Design"},
]

categories = ["All", "Programming", "Design"]

def format_views(count):
    if count >= 1000000:
        return f"{count / 1000000:.1f}M"
    elif count >= 1000:
        return f"{count / 1000:.0f}K"
    return str(count)

@c
def VideoGrid():
    search, set_search = us("")
    category, set_category = us("All")
    
    filtered = [v for v in videos 
                if search.lower() in v["title"].lower() 
                and (category == "All" or v["category"] == category)]
    
    return div(cn="video-grid-container",
        div(cn="search-section",
            input(cn="search-input", 
                  type="text",
                  ph="Search videos...",
                  value=search,
                  oh=lambda e: set_search(e.target.value)),
        ),
        div(cn="categories-section",
            *[button(
                cn="category-chip" + (" active" if cat == category else ""),
                oc=lambda c=cat: set_category(c),
                cat
              ) for cat in categories]
        ),
        div(cn="videos-grid",
            *[div(cn="video-card", key=v["title"],
                h3(cn="video-title", v["title"]),
                p(cn="video-channel", v["channel"]),
                p(cn="video-views", f"{format_views(v['views'])} views"),
                span(cn="video-category", v["category"]),
              ) for v in filtered]
        ) if filtered else div(cn="empty-state", p("No videos found. Try adjusting your filters.")),
    )
```
