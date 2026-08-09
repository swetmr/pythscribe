<!-- exp=baseline-haiku task=macro_hello_card cond=ps sample=3 model=claude-haiku-4-5-20251001 cost_usd=0.0129416 -->
```python
"use client"

from pyths.react import component, use_state

@component
def HelloCard(title, subtitle=None):
    is_liked, set_is_liked = use_state(False)
    
    def toggle_like():
        set_is_liked(not is_liked)
    
    like_label = "Liked" if is_liked else "Like"
    
    return div(class_name="hello-card",
        h2(class_name="hello-card-title", title),
        subtitle and p(class_name="hello-card-subtitle", subtitle),
        button(class_name="hello-card-button", on_click=lambda: toggle_like(), like_label)
    )

@component
def App():
    return div(class_name="app",
        HelloCard("Welcome", "This is a greeting card"),
        HelloCard("Hello"),
        HelloCard("Another Card", "With a subtitle"),
    )
```
