<!-- exp=baseline-001 task=macro_video_grid cond=psc sample=3 model=claude-opus-4-8 cost_usd=0.034265 -->
```python
"use client"
R*

VIDEOS = [
    {"title": "Learn Python in 100 Seconds", "channel": "Fireship", "views": "2.1M", "category": "Coding"},
    {"title": "Building a REST API", "channel": "Traversy Media", "views": "845K", "category": "Coding"},
    {"title": "Lo-fi Beats to Study To", "channel": "ChilledCow", "views": "12M", "category": "Music"},
    {"title": "Guitar Solo Compilation", "channel": "RockDaily", "views": "560K", "category": "Music"},
    {"title": "Perfect Scrambled Eggs", "channel": "Chef John", "views": "3.4M", "category": "Cooking"},
    {"title": "Neapolitan Pizza at Home", "channel": "Vincenzo", "views": "1.2M", "category": "Cooking"},
    {"title": "Champions League Highlights", "channel": "UEFA", "views": "8.7M", "category": "Sports"},
    {"title": "Top 10 Marathon Tips", "channel": "RunFast", "views": "230K", "category": "Sports"},
    {"title": "React Hooks Deep Dive", "channel": "Fireship", "views": "990K", "category": "Coding"},
]

def categories_of(videos):
    seen = ["All"]
    for v in videos:
        if v["category"] not in seen:
            seen.append(v["category"])
    return seen

@c
def VideoGrid():
    query, set_query = us("")
    active, set_active = us("All")

    chips = categories_of(VIDEOS)
    q = query.lower()
    results = [
        v for v in VIDEOS
        if q in v["title"].lower() and (active == "All" or v["category"] == active)
    ]

    return div(cn="video-grid",
        div(cn="search-bar",
            input(cn="search-input", value=query,
                  oh=lambda e: set_query(e.target.value),
                  ph="Search videos..."),
        ),
        div(cn="chip-row",
            *[button(key=c, cn=("chip active" if c == active else "chip"),
                     oc=lambda c=c: set_active(c), c)
              for c in chips]),
        (div(cn="empty-state", p("No videos match your search."))
         if len(results) == 0
         else div(cn="card-grid",
            *[div(key=v["title"], cn="video-card",
                h3(cn="card-title", v["title"]),
                p(cn="card-channel", v["channel"]),
                p(cn="card-views", f"{v['views']} views"),
              ) for v in results])),
    )
```
