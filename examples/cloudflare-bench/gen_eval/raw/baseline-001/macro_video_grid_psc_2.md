<!-- exp=baseline-001 task=macro_video_grid cond=psc sample=2 model=claude-opus-4-8 cost_usd=0.038955 -->
```python
"use client"
R*

def fmt_views(n):
    if n >= 1000000:
        return f"{n / 1000000:.1f}M views"
    if n >= 1000:
        return f"{n / 1000:.0f}K views"
    return f"{n} views"

VIDEOS = [
    {"id": 1, "title": "Build a Web App in 100 Seconds", "channel": "Fireship", "views": 2400000, "category": "Tech"},
    {"id": 2, "title": "Lo-Fi Beats to Study To", "channel": "ChilledCow", "views": 18000000, "category": "Music"},
    {"id": 3, "title": "10-Minute Full Body Workout", "channel": "FitLife", "views": 850000, "category": "Fitness"},
    {"id": 4, "title": "One-Pan Pasta Recipe", "channel": "Tasty Kitchen", "views": 3200000, "category": "Cooking"},
    {"id": 5, "title": "Understanding Quantum Computing", "channel": "SciSimple", "views": 640000, "category": "Tech"},
    {"id": 6, "title": "Guitar Solo Masterclass", "channel": "StringTheory", "views": 210000, "category": "Music"},
    {"id": 7, "title": "Morning Yoga Flow", "channel": "FitLife", "views": 1100000, "category": "Fitness"},
    {"id": 8, "title": "Perfect Sourdough Bread", "channel": "Tasty Kitchen", "views": 990000, "category": "Cooking"},
    {"id": 9, "title": "React vs Vue in 2026", "channel": "Fireship", "views": 1500000, "category": "Tech"},
    {"id": 10, "title": "Ambient Piano for Focus", "channel": "ChilledCow", "views": 5400000, "category": "Music"},
]

CATEGORIES = ["All", "Tech", "Music", "Fitness", "Cooking"]

@c
def VideoGrid():
    query, set_query = us("")
    category, set_category = us("All")

    q = query.strip().lower()
    filtered = [
        v for v in VIDEOS
        if (not q or q in v["title"].lower())
        and (category == "All" or v["category"] == category)
    ]

    return div(cn="video-grid",
        div(cn="vg-search",
            input(
                cn="vg-search-input",
                type="text",
                value=query,
                oh=lambda e: set_query(e.target.value),
                ph="Search videos...",
            ),
        ),
        div(cn="vg-chips",
            *[button(
                key=cat,
                cn=f"vg-chip {'active' if cat == category else ''}".strip(),
                oc=lambda c=cat: set_category(c),
                cat,
            ) for cat in CATEGORIES],
        ),
        div(cn="vg-cards",
            *[div(key=v["id"], cn="vg-card",
                div(cn="vg-thumb"),
                h3(cn="vg-title", v["title"]),
                p(cn="vg-channel", v["channel"]),
                p(cn="vg-views", fmt_views(v["views"])),
            ) for v in filtered],
        ) if filtered else div(cn="vg-empty",
            p("No videos match your search."),
        ),
    )
```
