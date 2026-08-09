<!-- exp=baseline-haiku task=macro_counter_panel cond=psc sample=4 model=claude-haiku-4-5-20251001 cost_usd=0.0160005 -->
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
    
    parity = "even" if count % 2 == 0 else "odd"
    
    return div(cn="counter-panel",
        h2("Counter"),
        p(cn="counter-display", f"Count: {count}"),
        p(cn="parity-message", f"The number is {parity}"),
        button(oc=increment, cn="btn-increment", "Increment"),
        button(oc=decrement, cn="btn-decrement", dis=(count == 0), "Decrement"),
        button(oc=reset, cn="btn-reset", "Reset"),
    )
```
