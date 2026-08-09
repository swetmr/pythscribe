<!-- exp=macrov2-haiku task=macro_counter_panel cond=psc sample=1 model=claude-haiku-4-5-20251001 cost_usd=0.0194456 -->
```python
"use client"
R*

@c
def CounterPanel():
    count, set_count = us(0)
    
    def increment():
        set_count(count + 1)
    
    def decrement():
        if count > 0:
            set_count(count - 1)
    
    def reset():
        set_count(0)
    
    parity = "even" if count % 2 == 0 else "odd"
    
    return div(cn="counter-panel",
        div(cn="counter-display",
            h2(str(count)),
            p(cn="parity", parity),
        ),
        div(cn="counter-controls",
            button(oc=increment, "Increment"),
            button(oc=decrement, dis=(count == 0), "Decrement"),
            button(oc=reset, "Reset"),
        ),
    )
```
