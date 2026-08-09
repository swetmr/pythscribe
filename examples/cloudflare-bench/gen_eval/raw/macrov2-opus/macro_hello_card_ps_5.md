<!-- exp=macrov2-opus task=macro_hello_card cond=ps sample=5 model=claude-opus-4-8 cost_usd=0.0186665 -->
```python
"use client"

from pyths.react import component, use_state


@component
def HelloCard(title, subtitle=None):
    liked, set_liked = use_state(False)

    return div(class_name="hello-card",
        h2(class_name="hello-card__title", title),
        p(class_name="hello-card__subtitle", subtitle) if subtitle else None,
        button(
            class_name="hello-card__like" + (" is-liked" if liked else ""),
            on_click=lambda: set_liked(not liked),
            "Liked" if liked else "Like",
        ),
    )


@component
def App():
    return div(class_name="app",
        HelloCard(title="Hello, World", subtitle="Welcome to PythScribe"),
        HelloCard(title="No Subtitle Here"),
    )
```
