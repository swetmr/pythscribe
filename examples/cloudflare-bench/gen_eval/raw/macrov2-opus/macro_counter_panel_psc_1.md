<!-- exp=macrov2-opus task=macro_counter_panel cond=psc sample=1 model=claude-opus-4-8 cost_usd=0.063754 -->
```python
"use client"
R*

@c
def CounterPanel():
    count, set_count = us(0)
    parity = "even" if count % 2 == 0 else "odd"
    return div(cn="counter-panel",
        h2("Counter"),
        p(cn="value", f"{count}"),
        p(cn="parity", parity),
        div(cn="buttons",
            button(oc=lambda: set_count(count + 1), "Increment"),
            button(oc=lambda: set_count(count - 1), dis=(count == 0), "Decrement"),
            button(oc=lambda: set_count(0), "Reset"),
        ),
    )
```
