<!-- exp=baseline-sonnet task=macro_counter_panel cond=ps sample=2 model=claude-sonnet-5 cost_usd=0.0149253 -->
```python
"use client"

from pyths.react import component, use_state

def parity_label(value):
    return "even" if value % 2 == 0 else "odd"

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
        h2(class_name="counter-panel__title", "Counter"),
        p(class_name="counter-panel__value", f"{count}"),
        p(class_name="counter-panel__parity", f"{parity_label(count)}"),
        div(class_name="counter-panel__controls",
            button(class_name="counter-panel__button", on_click=lambda: decrement(),
                   disabled=count == 0, "Decrement"),
            button(class_name="counter-panel__button", on_click=lambda: increment(), "Increment"),
            button(class_name="counter-panel__button", on_click=lambda: reset(), "Reset"),
        ),
    )
```
