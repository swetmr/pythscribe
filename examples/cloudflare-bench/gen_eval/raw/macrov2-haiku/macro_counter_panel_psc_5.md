<!-- exp=macrov2-haiku task=macro_counter_panel cond=psc sample=5 model=claude-haiku-4-5-20251001 cost_usd=0.012053000000000001 -->
```python
"use client"
R*

@c
def CounterPanel():
    count, set_count = us(0)
    
    is_even = count % 2 == 0
    parity = "even" if is_even else "odd"
    
    return div(cn="counter-panel",
        h2("Counter Panel"),
        p(f"Count: {count}"),
        p(f"The number is {parity}"),
        div(cn="button-group",
            button(oc=lambda: set_count(count + 1), "Increment"),
            button(oc=lambda: set_count(count - 1), dis=(count == 0), "Decrement"),
            button(oc=lambda: set_count(0), "Reset"),
        ),
    )
```
