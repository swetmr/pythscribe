<!-- exp=baseline-001 task=macro_video_grid cond=psc sample=5 model=claude-opus-4-8 cost_usd=0.03543999999999999 -->
```python
"use client"
R*

VIDEOS = [
    {"title": "Learn PythScribe in 10 Minutes", "channel": "CodeCraft", "views": "1.2M views", "category": "Education"},
    {"title": "Epic Mountain Bike Downhill Run", "channel": "TrailBlazers", "views": "845K views", "category": "Sports"},
    {"title": "Lo-fi Beats to Study To", "channel": "ChillWave", "views": "3.4M views", "category": "Music"},
    {"title": "Building a React Grid from Scratch", "channel": "DevDaily", "views": "512K views", "category": "Education"},
    {"title": "Top 10 Cooking Hacks", "channel": "KitchenPro", "views": "2.1M views", "category": "Cooking"},
    {"title": "Relaxing Piano Music for Sleep", "channel": "SereneSound", "views": "6.8M views", "category": "Music"},
    {"title": "Marathon Training Full Guide", "channel": "RunFast", "views": "290K views", "category": "Sports"},
    {"title": "One-Pan Pasta Recipe", "channel": "KitchenPro", "views": "1.9M views", "category": "Cooking"},
    {"title": "Understanding Async in JavaScript", "channel": "DevDaily", "views": "733K views", "category": "Education"},
]

CATEGORIES = ["All", "Education", "Sports", "Music", "Cooking"]

def matches(video, query, category):
    hit_title = query.lower() in video["title"].lower()
    hit_cat = category == "All" or video["category"] == category
    return hit_title and hit_cat

@c
def VideoGrid():
    query, set_query = us("")
    category, set_category = us("All")

    results = [v for v in VIDEOS if matches(v, query, category)]

    return div(cn="video-grid",
        div(cn="search-bar",
            input(cn="search-input", type="text", value=query,
                  oh=lambda e: set_query(e.target.value),
                  ph="Search videos..."),
        ),
        div(cn="chip-row",
            *[button(key=cat,
                     cn=f"chip active" if cat == category else "chip",
                     oc=lambda c=cat: set_category(c),
                     cat)
              for cat in CATEGORIES]),
        (div(cn="grid",
            *[div(key=v["title"], cn="video-card",
                div(cn="thumbnail"),
                h3(cn="video-title", v["title"]),
                p(cn="video-channel", v["channel"]),
                p(cn="video-views", v["views"]),
            ) for v in results])
         if results else
         div(cn="empty-state", p("No videos match your search."))),
    )
```
