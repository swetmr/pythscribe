<!-- exp=baseline-001 task=macro_hello_card cond=ps sample=4 model=claude-opus-4-8 cost_usd=0.0172045 -->
```python
"use client"

from pyths.react import component, use_state

CARDS = [
    {"title": "Welcome", "subtitle": "Glad you're here"},
    {"title": "Standalone"},
]


@component
def HelloCard(title, subtitle=None):
    liked, set_liked = use_state(False)

    label = "Liked" if liked else "Like"

    return div(class_name="hello-card",
        h2(class_name="hello-card__title", title),
        p(class_name="hello-card__subtitle", subtitle) if subtitle else None,
        button(class_name="hello-card__like",
               on_click=lambda: set_liked(not liked),
               label),
    )
```
