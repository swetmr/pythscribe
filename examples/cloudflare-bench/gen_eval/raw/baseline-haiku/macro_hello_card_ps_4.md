<!-- exp=baseline-haiku task=macro_hello_card cond=ps sample=4 model=claude-haiku-4-5-20251001 cost_usd=0.0141866 -->
```python
from pyths.react import component, use_state

@component
def HelloCard(title, subtitle=None):
    liked, set_liked = use_state(False)
    
    def toggle_like():
        set_liked(not liked)
    
    return div(class_name="card",
        h2(class_name="card-title", title),
        (p(class_name="card-subtitle", subtitle) if subtitle else None),
        button(on_click=lambda: toggle_like(), class_name="like-button",
               "Liked" if liked else "Like")
    )
```
