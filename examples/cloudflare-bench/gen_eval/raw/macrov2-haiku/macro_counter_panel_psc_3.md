<!-- exp=macrov2-haiku task=macro_counter_panel cond=psc sample=3 model=claude-haiku-4-5-20251001 cost_usd=0.017413 -->
```python
"use client"
R*

@c
def CounterPanel():
    count, set_count = us(0)
    
    is_even = count % 2 == 0
    parity = "even" if is_even else "odd"
    
    return div(cn="counter-panel",
        div(cn="display",
            h2("Counter"),
            p(cn="count", str(count)),
            p(cn="status", f"The number is {parity}"),
        ),
        div(cn="controls",
            button(oc=lambda: set_count(count + 1), cn="btn", "Increment"),
            button(oc=lambda: set_count(count - 1), cn="btn", dis=(count == 0), "Decrement"),
            button(oc=lambda: set_count(0), cn="btn", "Reset"),
        ),
    )
```
