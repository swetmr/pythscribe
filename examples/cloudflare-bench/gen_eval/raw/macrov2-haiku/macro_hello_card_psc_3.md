<!-- exp=macrov2-haiku task=macro_hello_card cond=psc sample=3 model=claude-haiku-4-5-20251001 cost_usd=0.0361833 -->
```python
"use client"
R*

@c
def HelloCard(title, subtitle=None):
    liked, set_liked = us(False)
    
    return div(cn="card",
        h2(title),
        p(subtitle) if subtitle else None,
        button(oc=lambda: set_liked(not liked), cn="like-button", "Liked" if liked else "Like"),
    )
```
