<!-- exp=baseline-001 task=macro_counter_panel cond=ps sample=2 model=claude-opus-4-8 cost_usd=0.019821 -->
```python
"use client"

from pyths.react import component, use_state


def parity_label(n):
    return "even" if n % 2 == 0 else "odd"


@component
def CounterPanel():
    count, set_count = use_state(0)

    def increment():
        set_count(count + 1)

    def decrement():
        set_count(count - 1)

    def reset():
        set_count(0)

    return div(class_name="counter-panel",
        h2(class_name="counter-title", "Counter"),
        p(class_name="counter-value", str(count)),
        p(class_name="counter-parity", f"{parity_label(count)}"),
        div(class_name="counter-buttons",
            button(class_name="btn btn-increment", on_click=lambda: increment(), "Increment"),
            button(class_name="btn btn-decrement", on_click=lambda: decrement(),
                   disabled=(count == 0), "Decrement"),
            button(class_name="btn btn-reset", on_click=lambda: reset(), "Reset"),
        ),
    )
```
