<!-- exp=baseline-001 task=macro_counter_panel cond=psc sample=1 model=claude-opus-4-8 cost_usd=0.0616355 -->
```python
"use client"
R*

@c
def CounterPanel():
    count, set_count = us(0)
    parity = "even" if count % 2 == 0 else "odd"
    return div(cn="counter-panel",
        h2("Counter"),
        p(cn="counter-value", f"{count}"),
        p(cn="counter-parity", parity),
        div(cn="counter-buttons",
            button(cn="btn-inc", oc=lambda: set_count(count + 1), "Increment"),
            button(cn="btn-dec", dis=count == 0,
                   oc=lambda: set_count(count - 1), "Decrement"),
            button(cn="btn-reset", oc=lambda: set_count(0), "Reset"),
        ),
    )
```
