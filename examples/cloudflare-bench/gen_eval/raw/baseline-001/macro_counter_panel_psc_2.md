<!-- exp=baseline-001 task=macro_counter_panel cond=psc sample=2 model=claude-opus-4-8 cost_usd=0.016136 -->
```python
"use client"
R*

@c
def CounterPanel():
    count, set_count = us(0)
    parity = "even" if count % 2 == 0 else "odd"
    return div(cn="counter-panel",
        h2(cn="counter-value", f"{count}"),
        p(cn="counter-parity", parity),
        div(cn="counter-buttons",
            button(cn="btn btn-inc", oc=lambda: set_count(count + 1), "+"),
            button(cn="btn btn-dec", dis=(count == 0),
                   oc=lambda: set_count(count - 1), "-"),
            button(cn="btn btn-reset", oc=lambda: set_count(0), "Reset"),
        ),
    )
```
