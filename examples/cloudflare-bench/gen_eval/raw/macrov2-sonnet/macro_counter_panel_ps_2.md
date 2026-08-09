<!-- exp=macrov2-sonnet task=macro_counter_panel cond=ps sample=2 model=claude-sonnet-5 cost_usd=0.01482 -->
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
        h2(class_name="counter-value", f"{count}"),
        p(class_name="counter-message", f"{count} is {parity}"),
        div(class_name="counter-controls",
            button(class_name="counter-btn decrement", on_click=lambda: decrement(),
                   disabled=(count == 0), "Decrement"),
            button(class_name="counter-btn reset", on_click=lambda: reset(), "Reset"),
            button(class_name="counter-btn increment", on_click=lambda: increment(), "Increment"),
        ),
    )
```
