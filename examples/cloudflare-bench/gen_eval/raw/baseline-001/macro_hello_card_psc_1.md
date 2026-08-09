<!-- exp=baseline-001 task=macro_hello_card cond=psc sample=1 model=claude-opus-4-8 cost_usd=0.0655055 -->
```python
"use client"
R*

@c
def HelloCard(title, subtitle=None):
    liked, set_liked = us(False)
    return div(cn="hello-card",
        h2(cn="hello-card__title", title),
        p(cn="hello-card__subtitle", subtitle) if subtitle else None,
        button(cn="hello-card__like", oc=lambda: set_liked(not liked),
            "Liked" if liked else "Like"),
    )

fixtures = [
    {"title": "Welcome", "subtitle": "Glad you're here"},
    {"title": "Standalone", "subtitle": None},
]

@c
def App():
    return div(cn="hello-card-list",
        *[HelloCard(key=i, title=f["title"], subtitle=f["subtitle"])
          for i, f in enumerate(fixtures)],
    )
```
