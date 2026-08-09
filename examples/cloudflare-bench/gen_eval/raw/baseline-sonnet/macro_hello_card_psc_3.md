<!-- exp=baseline-sonnet task=macro_hello_card cond=psc sample=3 model=claude-sonnet-5 cost_usd=0.011099399999999999 -->
```python
"use client"
R*

@c
def HelloCard(title, subtitle=None):
    liked, set_liked = us(False)
    return div(cn="hello-card",
        h2(cn="hello-card-title", title),
        p(cn="hello-card-subtitle", subtitle) if subtitle else None,
        button(cn="hello-card-like", oc=lambda: set_liked(not liked), "Liked" if liked else "Like"),
    )
```
