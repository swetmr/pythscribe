<!-- exp=baseline-001 task=macro_video_grid cond=psc sample=1 model=claude-opus-4-8 cost_usd=0.0837155 -->
```python
"use client"
R*

VIDEOS = [
    {"title": "Learn PythScribe in 10 Minutes", "channel": "CodeCraft", "views": "1.2M", "category": "Tech"},
    {"title": "Lo-fi Beats to Study To", "channel": "ChillWave", "views": "8.4M", "category": "Music"},
    {"title": "Epic Boss Battle Highlights", "channel": "GameZone", "views": "540K", "category": "Gaming"},
    {"title": "One-Pan Pasta Recipe", "channel": "HomeCook", "views": "2.1M", "category": "Cooking"},
    {"title": "React State Management Deep Dive", "channel": "CodeCraft", "views": "310K", "category": "Tech"},
    {"title": "Guitar Cover: Autumn Leaves", "channel": "StringsAndThings", "views": "76K", "category": "Music"},
    {"title": "Speedrun World Record Attempt", "channel": "GameZone", "views": "1.9M", "category": "Gaming"},
    {"title": "30-Minute Full Body Workout", "channel": "FitLife", "views": "3.3M", "category": "Fitness"},
    {"title": "Sourdough Bread for Beginners", "channel": "HomeCook", "views": "990K", "category": "Cooking"},
    {"title": "Morning Yoga Flow", "channel": "FitLife", "views": "620K", "category": "Fitness"},
]

def categories():
    seen = []
    for v in VIDEOS:
        if v["category"] not in seen:
            seen.append(v["category"])
    return ["All", *seen]

@c
def VideoGrid():
    query, set_query = us("")
    active, set_active = us("All")

    q = query.strip().lower()
    filtered = [
        v for v in VIDEOS
        if q in v["title"].lower()
        and (active == "All" or v["category"] == active)
    ]

    def chip(cat):
        cls = "chip active" if cat == active else "chip"
        return button(key=cat, cn=cls, oc=lambda: set_active(cat), cat)

    return div(cn="video-grid",
        div(cn="search-bar",
            input(
                cn="search-input",
                type="text",
                value=query,
                oh=lambda e: set_query(e.target.value),
                ph="Search videos...",
            ),
        ),
        div(cn="chip-row",
            *[chip(cat) for cat in categories()],
        ),
        (
            div(cn="grid",
                *[
                    div(key=v["title"], cn="video-card",
                        div(cn="thumbnail"),
                        h3(cn="video-title", v["title"]),
                        p(cn="video-channel", v["channel"]),
                        p(cn="video-views", f"{v['views']} views"),
                    )
                    for v in filtered
                ],
            )
            if filtered
            else div(cn="empty-state",
                p("No videos match your search."),
            )
        ),
    )
```
