<!-- exp=macrov2-haiku task=macro_hello_card cond=ps sample=5 model=claude-haiku-4-5-20251001 cost_usd=0.0112188 -->
```python
"use client"

from pyths.react import component, use_state

@component
def HelloCard(title, subtitle=None):
    liked, set_liked = use_state(False)
    
    def toggle_like():
        set_liked(not liked)
    
    button_label = "Liked" if liked else "Like"
    
    return div(class_name="card",
        h2(title),
        subtitle and p(subtitle),
        button(on_click=toggle_like, button_label),
    )

@component
def App():
    return div(class_name="container",
        HelloCard("Welcome", "This is a subtitle"),
        HelloCard("Hello World"),
        HelloCard("Another Card", "With some description"),
    )
```
