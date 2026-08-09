<!-- exp=macrov2-haiku task=macro_counter_panel cond=psc sample=2 model=claude-haiku-4-5-20251001 cost_usd=0.010908000000000001 -->
```python
"use client"
R*

@c
def CounterPanel():
    count, set_count = us(0)
    
    return div(cn="counter-panel",
        h2("Counter"),
        p(f"Count: {count}"),
        p(f"Parity: {'even' if count % 2 == 0 else 'odd'}"),
        div(cn="button-group",
            button(oc=lambda: set_count(count - 1), dis=(count == 0), "Decrement"),
            button(oc=lambda: set_count(count + 1), "Increment"),
            button(oc=lambda: set_count(0), "Reset"),
        ),
    )
```
