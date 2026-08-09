<!-- exp=baseline-sonnet task=macro_counter_panel cond=psc sample=3 model=claude-sonnet-5 cost_usd=0.0153843 -->
```python
"use client"
R*

@c
def CounterPanel():
    count, set_count = us(0)

    def increment():
        set_count(count + 1)

    def decrement():
        set_count(count - 1)

    def reset():
        set_count(0)

    parity = "even" if count % 2 == 0 else "odd"

    return div(cn="counter-panel",
        h2(f"{count}"),
        p(cn="counter-message", f"{parity}"),
        div(cn="counter-controls",
            button(cn="btn-decrement", oc=lambda: decrement(), dis=count == 0, "-"),
            button(cn="btn-reset", oc=lambda: reset(), "Reset"),
            button(cn="btn-increment", oc=lambda: increment(), "+"),
        ),
    )
```
