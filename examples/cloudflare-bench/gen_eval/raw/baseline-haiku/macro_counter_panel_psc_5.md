<!-- exp=baseline-haiku task=macro_counter_panel cond=psc sample=5 model=claude-haiku-4-5-20251001 cost_usd=0.007480499999999999 -->
```python
"use client"
R*

@c
def CounterPanel():
    count, set_count = us(0)
    is_even = count % 2 == 0
    parity = "even" if is_even else "odd"
    
    return div(cn="counter-panel",
        div(cn="counter-display",
            h2(str(count)),
            p(f"The number is {parity}"),
        ),
        div(cn="counter-buttons",
            button(oc=lambda: set_count(count + 1), "Increment"),
            button(oc=lambda: set_count(count - 1), dis=(count == 0), "Decrement"),
            button(oc=lambda: set_count(0), "Reset"),
        ),
    )
```
