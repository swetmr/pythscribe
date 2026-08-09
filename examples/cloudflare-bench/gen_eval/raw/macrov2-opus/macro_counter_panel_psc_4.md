<!-- exp=macrov2-opus task=macro_counter_panel cond=psc sample=4 model=claude-opus-4-8 cost_usd=0.016493999999999998 -->
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
            button(cn="dec", oc=lambda: set_count(count - 1), dis=count == 0, "-"),
            button(cn="reset", oc=lambda: set_count(0), "Reset"),
            button(cn="inc", oc=lambda: set_count(count + 1), "+"),
        ),
    )
```
