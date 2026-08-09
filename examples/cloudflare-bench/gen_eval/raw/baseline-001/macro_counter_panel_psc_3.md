<!-- exp=baseline-001 task=macro_counter_panel cond=psc sample=3 model=claude-opus-4-8 cost_usd=0.016061 -->
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
        p(cn="parity", parity),
        div(cn="controls",
            button(cn="btn increment", oc=lambda: set_count(count + 1), "Increment"),
            button(cn="btn decrement", dis=count == 0,
                   oc=lambda: set_count(count - 1), "Decrement"),
            button(cn="btn reset", oc=lambda: set_count(0), "Reset"),
        ),
    )
```
