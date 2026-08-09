<!-- exp=macrov2-haiku task=macro_hello_card cond=ps sample=2 model=claude-haiku-4-5-20251001 cost_usd=0.019083799999999998 -->
```python
"use client"

from pyths.react import component, use_state

@component
def HelloCard(title, subtitle=None):
    liked, set_liked = use_state(False)
    
    def toggle_like():
        set_liked(not liked)
    
    return div(class_name="card",
        h2(class_name="card-title", title),
        p(class_name="card-subtitle", subtitle) if subtitle else None,
        button(class_name="like-button", on_click=toggle_like,
               "Liked" if liked else "Like"),
    )

@component
def App():
    return div(class_name="app",
        HelloCard("Welcome", "A greeting card"),
        HelloCard("Standalone Card"),
        HelloCard("Another Example", "With subtitle"),
    )
```
