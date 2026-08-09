<!-- exp=macrov2-sonnet task=macro_video_grid cond=psc sample=1 model=claude-sonnet-5 cost_usd=0.09451559999999999 -->
```python
"use client"
R*

VIDEOS = [
    {"id": 1, "title": "Lo-fi Beats to Study To", "channel": "ChillHop Music", "views": 1520000, "category": "Music"},
    {"id": 2, "title": "Top 10 Guitar Riffs of All Time", "channel": "GuitarWorld", "views": 843000, "category": "Music"},
    {"id": 3, "title": "Speedrunning Dark Souls in 30 Minutes", "channel": "SpeedGamer", "views": 2100000, "category": "Gaming"},
    {"id": 4, "title": "Building a Gaming PC from Scratch", "channel": "TechBuilds", "views": 980000, "category": "Tech"},
    {"id": 5, "title": "React Hooks Explained in 10 Minutes", "channel": "CodeCraft", "views": 456000, "category": "Tech"},
    {"id": 6, "title": "Perfect Homemade Pizza Dough", "channel": "Kitchen Basics", "views": 1230000, "category": "Cooking"},
    {"id": 7, "title": "5-Minute Pasta Recipes", "channel": "Kitchen Basics", "views": 675000, "category": "Cooking"},
    {"id": 8, "title": "Champions League Highlights", "channel": "Sports Central", "views": 3200000, "category": "Sports"},
    {"id": 9, "title": "How to Improve Your Free Throw", "channel": "HoopsTraining", "views": 312000, "category": "Sports"},
    {"id": 10, "title": "New Album Review: Best Tracks", "channel": "MusicReview", "views": 540000, "category": "Music"},
]

def select_category(chip, set_category):
    if chip == "All":
        set_category(None)
    else:
        set_category(chip)

@c
def VideoGrid():
    query, set_query = us("")
    category, set_category = us(None)

    categories = sorted(set(v["category"] for v in VIDEOS))
    chips = ["All"] + categories

    q = query.lower()
    filtered = [
        v for v in VIDEOS
        if q in v["title"].lower() and (category is None or v["category"] == category)
    ]

    return div(cn="video-grid-page",
        input(value=query, oh=lambda e: set_query(e.target.value),
              ph="Search videos...", cn="search-input"),
        div(cn="category-chips",
            *[
                button(
                    key=chip,
                    cn=("chip chip-active" if ((chip == "All" and category is None) or chip == category) else "chip"),
                    oc=lambda: select_category(chip, set_category),
                    chip,
                )
                for chip in chips
            ],
        ),
        div(cn="video-grid",
            *[
                div(key=v["id"], cn="video-card",
                    div(cn="thumbnail-placeholder"),
                    h3(cn="video-title", v["title"]),
                    p(cn="video-channel", v["channel"]),
                    p(cn="video-views", f"{v['views']:,} views"),
                )
                for v in filtered
            ],
        ) if filtered else p(cn="empty-state", "No videos found."),
    )
```
