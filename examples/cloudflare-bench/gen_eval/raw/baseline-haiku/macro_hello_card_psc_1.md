<!-- exp=baseline-haiku task=macro_hello_card cond=psc sample=1 model=claude-haiku-4-5-20251001 cost_usd=0.0140103 -->
```python
"use client"
R*

@c
def HelloCard(title, subtitle=None):
    liked, set_liked = us(False)
    
    def toggle_like():
        set_liked(not liked)
    
    label = "Liked" if liked else "Like"
    
    return div(cn="card",
        h2(title),
        subtitle and p(subtitle),
        button(oc=toggle_like, label),
    )
```
