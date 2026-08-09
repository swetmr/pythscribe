<!-- exp=baseline-001 task=macro_video_grid cond=psc sample=4 model=claude-opus-4-8 cost_usd=0.03813999999999999 -->
```python
"use client"
R*

VIDEOS = [
    {"title": "Learn Python in 10 Minutes", "channel": "CodeDaily", "views": "1.2M views", "category": "Education"},
    {"title": "Epic Guitar Solo Compilation", "channel": "RiffMaster", "views": "845K views", "category": "Music"},
    {"title": "Top 10 Goals of the Season", "channel": "SportsZone", "views": "3.4M views", "category": "Sports"},
    {"title": "Building a React App from Scratch", "channel": "DevHub", "views": "560K views", "category": "Education"},
    {"title": "Lo-Fi Beats to Relax To", "channel": "ChillWave", "views": "9.1M views", "category": "Music"},
    {"title": "Championship Final Highlights", "channel": "SportsZone", "views": "2.7M views", "category": "Sports"},
    {"title": "Funny Cat Fails 2026", "channel": "PetPlanet", "views": "12M views", "category": "Comedy"},
    {"title": "Cooking the Perfect Ramen", "channel": "TastyKitchen", "views": "678K views", "category": "Food"},
    {"title": "Stand-Up Comedy Night", "channel": "LaughTrack", "views": "1.9M views", "category": "Comedy"},
    {"title": "Vegan Meal Prep for the Week", "channel": "TastyKitchen", "views": "432K views", "category": "Food"},
]

def categories(videos):
    seen = []
    for v in videos:
        if v["category"] not in seen:
            seen.append(v["category"])
    return ["All", *seen]

@c
def VideoGrid():
    query, set_query = us("")
    active, set_active = us("All")

    q = query.strip().lower()
    matches = [
        v for v in VIDEOS
        if q in v["title"].lower()
        and (active == "All" or v["category"] == active)
    ]

    chips = categories(VIDEOS)

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
            *[
                button(
                    key=c,
                    cn="chip active" if c == active else "chip",
                    oc=lambda c=c: set_active(c),
                    c,
                )
                for c in chips
            ],
        ),
        div(cn="cards-empty", p("No videos match your search."))
        if not matches
        else div(cn="cards",
            *[
                div(key=v["title"], cn="video-card",
                    div(cn="thumbnail"),
                    h3(cn="video-title", v["title"]),
                    p(cn="video-channel", v["channel"]),
                    p(cn="video-views", v["views"]),
                )
                for v in matches
            ],
        ),
    )
```
