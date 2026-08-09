<!-- exp=baseline-sonnet task=macro_video_grid cond=psc sample=4 model=claude-sonnet-5 cost_usd=0.05305169999999999 -->
```python
"use client"
R*

VIDEOS = [
    {"title": "Lo-fi Beats to Study To", "channel": "ChillWave", "views": 2400000, "category": "Music"},
    {"title": "Speedrunning Every Boss", "channel": "PixelRush", "views": 850000, "category": "Gaming"},
    {"title": "Building a REST API in Rust", "channel": "CodeForge", "views": 42000, "category": "Tech"},
    {"title": "One-Pan Pasta in 15 Minutes", "channel": "Kitchen Sparks", "views": 1200000, "category": "Cooking"},
    {"title": "Live Coding: React from Scratch", "channel": "CodeForge", "views": 18000, "category": "Tech"},
    {"title": "Top 10 Indie Games of 2026", "channel": "PixelRush", "views": 610000, "category": "Gaming"},
    {"title": "Acoustic Guitar Cover Mix", "channel": "ChillWave", "views": 340000, "category": "Music"},
    {"title": "Sourdough Bread for Beginners", "channel": "Kitchen Sparks", "views": 980000, "category": "Cooking"},
    {"title": "Understanding Neural Networks", "channel": "CodeForge", "views": 205000, "category": "Tech"},
    {"title": "Marathon Training Week 1", "channel": "TrackDaily", "views": 76000, "category": "Sports"},
]

def format_views(n):
    if n >= 1000000:
        return f"{n / 1000000:.1f}M"
    elif n >= 1000:
        return f"{n / 1000:.1f}K"
    else:
        return str(n)

def video_card(v):
    return div(cn="video-card", key=v["title"],
        div(cn="thumbnail-placeholder"),
        div(cn="video-info",
            h3(cn="video-title", v["title"]),
            p(cn="video-channel", v["channel"]),
            p(cn="video-views", f"{format_views(v['views'])} views"),
        ),
    )

@c
def VideoGrid():
    query, set_query = us("")
    active_category, set_active_category = us(None)

    categories = sorted(set(v["category"] for v in VIDEOS))
    filtered = [
        v for v in VIDEOS
        if query.lower() in v["title"].lower()
        and (active_category is None or v["category"] == active_category)
    ]

    return div(cn="video-grid-page",
        input(value=query, oh=lambda e: set_query(e.target.value),
              ph="Search videos...", cn="search-input"),
        div(cn="chip-row",
            button(cn=f"chip {'chip-active' if active_category is None else ''}",
                   oc=lambda: set_active_category(None), "All"),
            *[button(key=cat,
                     cn=f"chip {'chip-active' if active_category == cat else ''}",
                     oc=lambda cat=cat: set_active_category(cat), cat)
              for cat in categories],
        ),
        div(cn="video-grid", *[video_card(v) for v in filtered])
            if filtered else div(cn="empty-state", "No videos found."),
    )
```
