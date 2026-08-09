<!-- exp=macrov2-opus task=macro_video_grid cond=psc sample=2 model=claude-opus-4-8 cost_usd=0.037173 -->
```python
"use client"
R*

VIDEOS = [
    {"title": "Learn PythScribe in 100 Seconds", "channel": "Fireship", "views": "1.2M", "category": "Coding"},
    {"title": "Building a REST API from Scratch", "channel": "Traversy Media", "views": "845K", "category": "Coding"},
    {"title": "Lo-fi Beats to Study To", "channel": "ChilledCow", "views": "12M", "category": "Music"},
    {"title": "Epic Orchestral Music Mix", "channel": "EpicMusicVN", "views": "3.4M", "category": "Music"},
    {"title": "The Universe Explained", "channel": "Kurzgesagt", "views": "9.1M", "category": "Science"},
    {"title": "How Black Holes Work", "channel": "Veritasium", "views": "5.6M", "category": "Science"},
    {"title": "One-Pan Weeknight Dinners", "channel": "Bon Appetit", "views": "678K", "category": "Cooking"},
    {"title": "Perfect Sourdough Every Time", "channel": "Joshua Weissman", "views": "2.1M", "category": "Cooking"},
    {"title": "10 Minute Morning Yoga", "channel": "Yoga With Adriene", "views": "4.3M", "category": "Fitness"},
    {"title": "Full Body Workout at Home", "channel": "Chloe Ting", "views": "7.8M", "category": "Fitness"},
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
    visible = [
        v for v in VIDEOS
        if (not q or q in v["title"].lower())
        and (active == "All" or v["category"] == active)
    ]

    chips = categories(VIDEOS)

    return div(cn="video-grid",
        div(cn="search-bar",
            input(
                cn="search-input",
                type="search",
                value=query,
                oh=lambda e: set_query(e.target.value),
                ph="Search videos...",
            ),
        ),
        div(cn="chip-row",
            *[
                button(
                    key=c,
                    cn=("chip active" if c == active else "chip"),
                    oc=lambda c=c: set_active(c),
                    c,
                )
                for c in chips
            ],
        ),
        div(cn="cards",
            *[
                div(key=v["title"], cn="card",
                    div(cn="thumb"),
                    h3(cn="card-title", v["title"]),
                    p(cn="card-channel", v["channel"]),
                    p(cn="card-views", f"{v['views']} views"),
                )
                for v in visible
            ],
        ) if visible else div(cn="empty-state",
            p("No videos match your search."),
        ),
    )
```
