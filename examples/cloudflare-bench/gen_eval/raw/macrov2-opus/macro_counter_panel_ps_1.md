<!-- exp=macrov2-opus task=macro_counter_panel cond=ps sample=1 model=claude-opus-4-8 cost_usd=0.052559 -->
```python
"use client"

from pyths.react import component, use_state


def parity(n):
    return "even" if n % 2 == 0 else "odd"


@component
def CounterPanel():
    count, set_count = use_state(0)

    return div(class_name="counter-panel",
        h2("Counter"),
        p(class_name="counter-value", str(count)),
        p(class_name="counter-parity", parity(count)),
        div(class_name="counter-buttons",
            button(class_name="btn btn-inc",
                   on_click=lambda: set_count(count + 1), "Increment"),
            button(class_name="btn btn-dec", disabled=count == 0,
                   on_click=lambda: set_count(count - 1), "Decrement"),
            button(class_name="btn btn-reset",
                   on_click=lambda: set_count(0), "Reset"),
        ),
    )
```
