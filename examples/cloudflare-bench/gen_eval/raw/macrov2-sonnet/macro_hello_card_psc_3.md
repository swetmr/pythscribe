<!-- exp=macrov2-sonnet task=macro_hello_card cond=psc sample=3 model=claude-sonnet-5 cost_usd=0.0138417 -->
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
```
