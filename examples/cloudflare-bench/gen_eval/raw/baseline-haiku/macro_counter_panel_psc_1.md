<!-- exp=baseline-haiku task=macro_counter_panel cond=psc sample=1 model=claude-haiku-4-5-20251001 cost_usd=0.0182993 -->
```python
"use client"
R*

@c
def CounterPanel():
    count, set_count = us(0)
    
    def increment():
        set_count(count + 1)
    
    def decrement():
        set_count(count - 1)
    
    def reset():
        set_count(0)
    
    is_even = count % 2 == 0
    parity = "even" if is_even else "odd"
    
    return div(cn="counter-panel",
        h2("Counter"),
        div(cn="counter-display",
            p(f"Count: {count}"),
            p(f"The number is {parity}"),
        ),
        div(cn="counter-buttons",
            button(oc=increment, "Increment"),
            button(oc=decrement, dis=(count == 0), "Decrement"),
            button(oc=reset, "Reset"),
        ),
    )
```
