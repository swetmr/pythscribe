<!-- exp=baseline-haiku task=macro_hello_card cond=ps sample=5 model=claude-haiku-4-5-20251001 cost_usd=0.0210966 -->
```python
"use client"

from pyths.react import component, use_state

@component
def HelloCard(title, subtitle=None):
    liked, set_liked = use_state(False)
    
    def toggle_like():
        set_liked(not liked)
    
    like_label = "Liked" if liked else "Like"
    
    return div(class_name="card",
        h2(class_name="card-title", title),
        (p(class_name="card-subtitle", subtitle) if subtitle else None),
        button(class_name="like-button", on_click=toggle_like, like_label),
    )

@component
def App():
    return div(class_name="app-container",
        HelloCard("Welcome to PythScribe", "Build interactive components with Python syntax"),
        HelloCard("A Simpler Card"),
        HelloCard("Fully Featured Card", "This card has both title and subtitle"),
    )
```
