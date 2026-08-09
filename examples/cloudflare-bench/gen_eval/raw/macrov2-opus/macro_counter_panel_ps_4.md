<!-- exp=macrov2-opus task=macro_counter_panel cond=ps sample=4 model=claude-opus-4-8 cost_usd=0.018033 -->
```python
"use client"

from pyths.react import component, use_state


@component
def CounterPanel():
    count, set_count = use_state(0)

    parity = "even" if count % 2 == 0 else "odd"

    return div(class_name="counter-panel",
        h2("Counter"),
        p(class_name="counter-value", str(count)),
        p(class_name="counter-parity", parity),
        div(class_name="counter-buttons",
            button(class_name="btn-decrement",
                   on_click=lambda: set_count(count - 1),
                   disabled=count == 0,
                   "-"),
            button(class_name="btn-increment",
                   on_click=lambda: set_count(count + 1),
                   "+"),
            button(class_name="btn-reset",
                   on_click=lambda: set_count(0),
                   "Reset"),
        ),
    )
```
