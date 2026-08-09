<!-- exp=baseline-haiku task=macro_hello_card cond=ps sample=1 model=claude-haiku-4-5-20251001 cost_usd=0.0193013 -->
```python
from pyths.react import component, use_state

@component
def HelloCard(title, subtitle=None):
    liked, set_liked = use_state(False)
    
    def toggle_like():
        set_liked(not liked)
    
    return div(class_name="card",
        h2(title),
        p(subtitle) if subtitle else None,
        button(on_click=toggle_like, class_name="like-button", "Liked" if liked else "Like")
    )
```
