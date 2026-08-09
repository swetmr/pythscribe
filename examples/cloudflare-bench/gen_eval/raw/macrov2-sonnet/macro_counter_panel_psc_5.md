<!-- exp=macrov2-sonnet task=macro_counter_panel cond=psc sample=5 model=claude-sonnet-5 cost_usd=0.0129366 -->
```python
"use client"
R*

@c
def CounterPanel():
    value, set_value = us(0)
    parity = "even" if value % 2 == 0 else "odd"

    return div(cn="counter-panel",
        h2(f"Count: {value}"),
        p(cn="counter-message", f"{parity}"),
        div(cn="counter-buttons",
            button(oc=lambda: set_value(value - 1), dis=value == 0, "Decrement"),
            button(oc=lambda: set_value(0), "Reset"),
            button(oc=lambda: set_value(value + 1), "Increment"),
        ),
    )
```
