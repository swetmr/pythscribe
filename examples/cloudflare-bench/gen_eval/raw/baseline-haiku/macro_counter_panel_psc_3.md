<!-- exp=baseline-haiku task=macro_counter_panel cond=psc sample=3 model=claude-haiku-4-5-20251001 cost_usd=0.0069654999999999995 -->
```python
"use client"
R*

@c
def CounterPanel():
    count, set_count = us(0)
    
    def is_even(n):
        return n % 2 == 0
    
    return div(cn="counter-panel",
        div(cn="counter-display",
            h1(str(count)),
            p(f"The number is {'even' if is_even(count) else 'odd'}"),
        ),
        div(cn="button-group",
            button(oc=lambda: set_count(count + 1), "Increment"),
            button(oc=lambda: set_count(count - 1), dis=count == 0, "Decrement"),
            button(oc=lambda: set_count(0), "Reset"),
        ),
    )
```
