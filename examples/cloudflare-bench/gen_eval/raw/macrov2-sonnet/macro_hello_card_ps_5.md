<!-- exp=macrov2-sonnet task=macro_hello_card cond=ps sample=5 model=claude-sonnet-5 cost_usd=0.013505099999999999 -->
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
        p(class_name="hello-card-subtitle", subtitle) if subtitle else None,
        button(class_name="hello-card-like-button", on_click=lambda: toggle_like(),
               "Liked" if liked else "Like"),
    )

@component
def App():
    return div(class_name="app",
        HelloCard(title="Welcome", subtitle="This is a sample card"),
        HelloCard(title="No Subtitle Card"),
    )
```
