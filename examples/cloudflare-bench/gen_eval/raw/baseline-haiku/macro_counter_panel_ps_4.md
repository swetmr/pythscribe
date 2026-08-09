<!-- exp=baseline-haiku task=macro_counter_panel cond=ps sample=4 model=claude-haiku-4-5-20251001 cost_usd=0.0156413 -->
```python
"use client"

from pyths.react import component, use_state

@component
def CounterPanel():
    count, set_count = use_state(0)
    
    def increment():
        set_count(count + 1)
    
    def decrement():
        set_count(count - 1)
    
    def reset():
        set_count(0)
    
    parity = "even" if count % 2 == 0 else "odd"
    
    return div(class_name="counter-panel",
        h2("Counter"),
        div(class_name="display",
            p(f"Count: {count}"),
            p(f"Status: {parity}"),
        ),
        div(class_name="controls",
            button(on_click=lambda: increment(), "Increment"),
            button(on_click=lambda: decrement(), disabled=count == 0, "Decrement"),
            button(on_click=lambda: reset(), "Reset"),
        ),
    )
```
