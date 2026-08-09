<!-- exp=baseline-001 task=macro_hello_card cond=ps sample=5 model=claude-opus-4-8 cost_usd=0.0204545 -->
```python
"use client"

from pyths.react import component, use_state

FIXTURE_CARDS = [
    {"title": "Welcome", "subtitle": "Glad you're here"},
    {"title": "Standalone", "subtitle": None},
]


@component
def HelloCard(title, subtitle=None):
    liked, set_liked = use_state(False)

    return div(class_name="hello-card",
        h2(class_name="hello-card__title", title),
        p(class_name="hello-card__subtitle", subtitle) if subtitle else None,
        button(
            class_name="hello-card__like" + (" hello-card__like--active" if liked else ""),
            on_click=lambda: set_liked(not liked),
            "Liked" if liked else "Like",
        ),
    )


@component
def App():
    return div(class_name="hello-card-list",
        *[HelloCard(key=i, title=card["title"], subtitle=card["subtitle"])
          for i, card in enumerate(FIXTURE_CARDS)],
    )
```
