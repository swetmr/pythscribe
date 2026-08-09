<!-- exp=baseline-001 task=macro_hello_card cond=ps sample=3 model=claude-opus-4-8 cost_usd=0.0161045 -->
```python
"use client"

from pyths.react import component, use_state


@component
def HelloCard(title, subtitle=None):
    liked, set_liked = use_state(False)

    return div(class_name="hello-card",
        h2(class_name="hello-card-title", title),
        p(class_name="hello-card-subtitle", subtitle) if subtitle else None,
        button(class_name="hello-card-like" + (" liked" if liked else ""),
               on_click=lambda: set_liked(not liked),
               "Liked" if liked else "Like"),
    )
```
