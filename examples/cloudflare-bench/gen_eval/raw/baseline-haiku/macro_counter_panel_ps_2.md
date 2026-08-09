<!-- exp=baseline-haiku task=macro_counter_panel cond=ps sample=2 model=claude-haiku-4-5-20251001 cost_usd=0.0173963 -->
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
    is_zero = count == 0
    
    return div(class_name="counter-panel",
        div(class_name="counter-display",
            h2("Counter Panel"),
            p(class_name="counter-value", str(count)),
            p(class_name="parity-message", f"The number is {parity}"),
        ),
        div(class_name="button-group",
            button(on_click=increment, "Increment"),
            button(on_click=decrement, disabled=is_zero, "Decrement"),
            button(on_click=reset, "Reset"),
        ),
    )
```
