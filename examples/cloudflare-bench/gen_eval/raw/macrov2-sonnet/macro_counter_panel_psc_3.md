<!-- exp=macrov2-sonnet task=macro_counter_panel cond=psc sample=3 model=claude-sonnet-5 cost_usd=0.0140166 -->
```python
"use client"
R*

@c
def CounterPanel():
    value, set_value = us(0)

    def increment():
        set_value(value + 1)

    def decrement():
        set_value(value - 1)

    def reset():
        set_value(0)

    parity = "even" if value % 2 == 0 else "odd"

    return div(cn="counter-panel",
        h2(cn="counter-value", f"{value}"),
        p(cn="counter-message", f"{value} is {parity}"),
        div(cn="counter-controls",
            button(cn="btn btn-decrement", oc=lambda: decrement(), dis=value == 0, "-"),
            button(cn="btn btn-reset", oc=lambda: reset(), "Reset"),
            button(cn="btn btn-increment", oc=lambda: increment(), "+"),
        ),
    )
```
