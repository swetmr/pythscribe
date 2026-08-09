<!-- exp=baseline-001 task=macro_counter_panel cond=psc sample=4 model=claude-opus-4-8 cost_usd=0.016111 -->
```python
"use client"
R*

@c
def CounterPanel():
    count, set_count = us(0)
    parity = "even" if count % 2 == 0 else "odd"
    return div(cn="counter-panel",
        h2("Counter"),
        p(cn="count-value", f"{count}"),
        p(cn="parity-message", parity),
        div(cn="button-row",
            button(cn="btn-decrement", oc=lambda: set_count(count - 1),
                   dis=(count == 0), "-"),
            button(cn="btn-reset", oc=lambda: set_count(0), "Reset"),
            button(cn="btn-increment", oc=lambda: set_count(count + 1), "+"),
        ),
    )
```
