<!-- exp=baseline-sonnet task=macro_hello_card cond=ps sample=2 model=claude-sonnet-5 cost_usd=0.0120204 -->
```python
"use client"

from pyths.react import component, use_state

@component
def HelloCard(title, subtitle=None):
    liked, set_liked = use_state(False)

    def toggle_like():
        set_liked(not liked)

    return div(class_name="hello-card",
        h2(class_name="hello-card-title", title),
        subtitle and p(class_name="hello-card-subtitle", subtitle),
        button(class_name="hello-card-like-button", on_click=lambda: toggle_like(),
               "Liked" if liked else "Like"),
    )
```
