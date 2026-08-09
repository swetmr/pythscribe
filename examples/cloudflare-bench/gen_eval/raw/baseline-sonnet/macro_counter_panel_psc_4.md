<!-- exp=baseline-sonnet task=macro_counter_panel cond=psc sample=4 model=claude-sonnet-5 cost_usd=0.0170193 -->
```python
"use client"
R*

@c
def CounterPanel():
    count, set_count = us(0)

    def reset():
        set_count(0)

    parity = "even" if count % 2 == 0 else "odd"

    return div(cn="counter-panel",
        h2(f"Count: {count}"),
        p(cn="counter-message", f"The current value is {parity}"),
        div(cn="counter-buttons",
            button(cn="btn-decrement", dis=count == 0,
                   oc=lambda: set_count(count - 1), "Decrement"),
            button(cn="btn-increment", oc=lambda: set_count(count + 1), "Increment"),
            button(cn="btn-reset", oc=lambda: reset(), "Reset"),
        ),
    )
```
