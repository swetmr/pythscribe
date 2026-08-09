<!-- exp=macrov2-opus task=macro_hello_card cond=ps sample=2 model=claude-opus-4-8 cost_usd=0.07717 -->
```python
"use client"

from pyths.react import component, use_state


@component
def HelloCard(title, subtitle=None):
    liked, set_liked = use_state(False)

    return div(class_name="hello-card",
        h2(class_name="hello-card-title", title),
        p(class_name="hello-card-subtitle", subtitle) if subtitle else None,
        button(class_name="hello-card-like",
               on_click=lambda: set_liked(not liked),
               "Liked" if liked else "Like"),
    )


@component
def App():
    return div(class_name="hello-card-list",
        HelloCard(title="Welcome", subtitle="Glad you're here"),
        HelloCard(title="No subtitle here"),
    )
```
