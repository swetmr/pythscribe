<!-- exp=baseline-haiku task=macro_counter_panel cond=ps sample=3 model=claude-haiku-4-5-20251001 cost_usd=0.0121113 -->
```python
"use client"

from pyths.react import component, use_state

@component
def CounterPanel():
    count, set_count = use_state(0)
    
    parity = "even" if count % 2 == 0 else "odd"
    
    return div(class_name="counter-panel",
        h2("Counter Panel"),
        div(class_name="counter-display",
            p(f"Count: {count}"),
            p(f"Status: {parity}"),
        ),
        div(class_name="button-group",
            button(on_click=lambda: set_count(count + 1), "Increment"),
            button(on_click=lambda: set_count(count - 1), disabled=count == 0, "Decrement"),
            button(on_click=lambda: set_count(0), "Reset"),
        ),
    )
```
