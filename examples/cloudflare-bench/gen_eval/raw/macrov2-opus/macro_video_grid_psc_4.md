<!-- exp=macrov2-opus task=macro_video_grid cond=psc sample=4 model=claude-opus-4-8 cost_usd=0.039173 -->
```python
"use client"
R*

VIDEOS = [
    {"id": 1, "title": "Learn PythScribe in 10 Minutes", "channel": "CodeCraft", "views": "1.2M views", "category": "Education"},
    {"id": 2, "title": "Epic Mountain Drone Footage", "channel": "WildLens", "views": "845K views", "category": "Travel"},
    {"id": 3, "title": "Top 10 Lo-Fi Beats to Study", "channel": "ChillWave", "views": "3.4M views", "category": "Music"},
    {"id": 4, "title": "Building a React Grid from Scratch", "channel": "CodeCraft", "views": "612K views", "category": "Education"},
    {"id": 5, "title": "Street Food Tour of Tokyo", "channel": "TasteBuds", "views": "2.1M views", "category": "Food"},
    {"id": 6, "title": "The Physics of Black Holes", "channel": "SciSpark", "views": "980K views", "category": "Science"},
    {"id": 7, "title": "Acoustic Guitar Cover Session", "channel": "ChillWave", "views": "430K views", "category": "Music"},
    {"id": 8, "title": "Ultimate Home Workout Routine", "channel": "FitFlow", "views": "1.7M views", "category": "Fitness"},
    {"id": 9, "title": "Cooking Pasta Like a Pro", "channel": "TasteBuds", "views": "756K views", "category": "Food"},
    {"id": 10, "title": "Exploring the Deep Ocean", "channel": "SciSpark", "views": "1.1M views", "category": "Science"},
]

def categories():
    seen = []
    for v in VIDEOS:
        if v["category"] not in seen:
            seen.append(v["category"])
    return ["All", *seen]

@c
def VideoGrid():
    search, set_search = us("")
    active_cat, set_active_cat = us("All")

    q = search.strip().lower()
    visible = [
        v for v in VIDEOS
        if q in v["title"].lower()
        and (active_cat == "All" or v["category"] == active_cat)
    ]

    return div(cn="video-grid-page",
        header(cn="vg-header",
            h1(cn="vg-logo", "MyTube"),
            input(cn="vg-search", type="text", value=search,
                  ph="Search videos...",
                  oh=lambda e: set_search(e.target.value)),
        ),
        div(cn="vg-chips",
            *[button(
                key=c,
                cn=("vg-chip active" if c == active_cat else "vg-chip"),
                oc=lambda: set_active_cat(c),
                c,
            ) for c in categories()]
        ),
        (div(cn="vg-cards",
            *[div(key=v["id"], cn="vg-card",
                div(cn="vg-thumb"),
                h3(cn="vg-title", v["title"]),
                p(cn="vg-channel", v["channel"]),
                p(cn="vg-views", v["views"]),
            ) for v in visible]
        ) if visible else div(cn="vg-empty",
            p("No videos match your search."),
        )),
    )
```
