<!-- exp=macrov2-haiku task=macro_hello_card cond=ps sample=3 model=claude-haiku-4-5-20251001 cost_usd=0.0108938 -->
```python
"use client"

from pyths.react import component, use_state

@component
def HelloCard(title, subtitle=None):
    liked, set_liked = use_state(False)
    
    def toggle_like():
        set_liked(not liked)
    
    button_text = "Liked" if liked else "Like"
    
    return div(class_name="hello-card",
        h2(class_name="hello-card-title", title),
        p(class_name="hello-card-subtitle", subtitle) if subtitle else None,
        button(class_name="hello-card-button", on_click=toggle_like, button_text),
    )

@component
def App():
    return div(class_name="app",
        HelloCard("Welcome", "This is a greeting card"),
        HelloCard("Another Card"),
    )
```
