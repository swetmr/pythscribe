<!-- exp=baseline-001 task=macro_hello_card cond=psc sample=3 model=claude-opus-4-8 cost_usd=0.015269500000000002 -->
```python
"use client"
R*

@c
def HelloCard(title, subtitle=None):
    liked, set_liked = us(False)
    return div(cn="hello-card",
        h2(cn="hello-card-title", title),
        p(cn="hello-card-subtitle", subtitle) if subtitle else None,
        button(cn="like-btn", oc=lambda: set_liked(not liked),
            "Liked" if liked else "Like"),
    )
```
