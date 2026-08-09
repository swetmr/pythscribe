<!-- exp=baseline-sonnet task=macro_counter_panel cond=psc sample=1 model=claude-sonnet-5 cost_usd=0.041064 -->
```python
"use client"
R*

@c
def CounterPanel():
    count, set_count = us(0)

    def increment():
        set_count(count + 1)

    def decrement():
        if count > 0:
            set_count(count - 1)

    def reset():
        set_count(0)

    parity = "even" if count % 2 == 0 else "odd"

    return div(cn="counter-panel",
        h2(f"Count: {count}"),
        p(cn="counter-message", f"{parity}"),
        div(cn="counter-buttons",
            button(cn="btn-decrement", oc=lambda: decrement(), dis=count == 0, "-"),
            button(cn="btn-reset", oc=lambda: reset(), "Reset"),
            button(cn="btn-increment", oc=lambda: increment(), "+"),
        ),
    )
```
