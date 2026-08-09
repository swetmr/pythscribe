<!-- exp=macrov2-haiku task=macro_counter_panel cond=ps sample=5 model=claude-haiku-4-5-20251001 cost_usd=0.016088500000000002 -->
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
    
    is_even = count % 2 == 0
    parity = "even" if is_even else "odd"
    
    return div(class_name="counter-panel",
        h2("Counter Panel"),
        div(class_name="counter-display",
            p(f"Current count: {count}"),
            p(f"The number is {parity}"),
        ),
        div(class_name="button-group",
            button(on_click=lambda: increment(), "Increment"),
            button(on_click=lambda: decrement(), disabled=count == 0, "Decrement"),
            button(on_click=lambda: reset(), "Reset"),
        ),
    )
```
