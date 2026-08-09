<!-- exp=macrov2-opus task=macro_counter_panel cond=psc sample=3 model=claude-opus-4-8 cost_usd=0.015219 -->
```python
"use client"
R*

@c
def CounterPanel():
    count, set_count = us(0)
    parity = "even" if count % 2 == 0 else "odd"
    return div(cn="counter-panel",
        h2("Counter"),
        p(cn="value", str(count)),
        p(cn="parity", parity),
        div(cn="controls",
            button(oc=lambda: set_count(count - 1), dis=count == 0, "-"),
            button(oc=lambda: set_count(0), "Reset"),
            button(oc=lambda: set_count(count + 1), "+"),
        ),
    )
```
