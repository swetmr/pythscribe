<!-- exp=baseline-sonnet task=macro_counter_panel cond=psc sample=5 model=claude-sonnet-5 cost_usd=0.013959300000000001 -->
```python
"use client"
R*

@c
def CounterPanel():
    count, set_count = us(0)

    parity = "even" if count % 2 == 0 else "odd"

    return div(cn="counter-panel",
        h2(f"Count: {count}"),
        p(f"The value is {parity}"),
        button(oc=lambda: set_count(count + 1), "Increment"),
        button(oc=lambda: set_count(count - 1), dis=count == 0, "Decrement"),
        button(oc=lambda: set_count(0), "Reset"),
    )
```
