<!-- exp=baseline-haiku task=macro_hello_card cond=psc sample=2 model=claude-haiku-4-5-20251001 cost_usd=0.007140800000000001 -->
```python
"use client"
R*

@c
def HelloCard(title, subtitle=None):
    liked, set_liked = us(False)
    
    return div(cn="card",
        h2(title),
        subtitle and p(subtitle),
        button(oc=lambda: set_liked(not liked), 
               "Liked" if liked else "Like"),
    )
```
