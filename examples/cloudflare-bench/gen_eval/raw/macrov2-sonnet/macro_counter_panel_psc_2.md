<!-- exp=macrov2-sonnet task=macro_counter_panel cond=psc sample=2 model=claude-sonnet-5 cost_usd=0.015366600000000001 -->
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
        p(cn="parity-message", f"{count} is {parity}"),
        div(cn="button-row",
            button(cn="btn-decrement", dis=count == 0,
                   oc=lambda: set_count(count - 1), "-"),
            button(cn="btn-reset", oc=lambda: reset(), "Reset"),
            button(cn="btn-increment", oc=lambda: set_count(count + 1), "+"),
        ),
    )
```
