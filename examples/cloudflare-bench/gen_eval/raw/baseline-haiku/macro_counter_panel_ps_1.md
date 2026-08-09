<!-- exp=baseline-haiku task=macro_counter_panel cond=ps sample=1 model=claude-haiku-4-5-20251001 cost_usd=0.0253753 -->
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
        h2("Counter"),
        div(class_name="counter-display",
            p(f"Current count: {count}"),
            p(f"The number is {parity}"),
        ),
        div(class_name="counter-buttons",
            button(on_click=increment, "Increment"),
            button(on_click=decrement, disabled=count == 0, "Decrement"),
            button(on_click=reset, "Reset"),
        ),
    )
```
