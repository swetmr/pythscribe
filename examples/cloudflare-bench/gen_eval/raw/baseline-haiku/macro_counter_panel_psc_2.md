<!-- exp=baseline-haiku task=macro_counter_panel cond=psc sample=2 model=claude-haiku-4-5-20251001 cost_usd=0.0107805 -->
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
    can_decrement = count > 0
    
    return div(cn="counter-panel",
        div(cn="display",
            h1(str(count)),
        ),
        div(cn="parity",
            p(f"The number is {parity}"),
        ),
        div(cn="buttons",
            button(oc=increment, "Increment"),
            button(oc=decrement, dis=not can_decrement, "Decrement"),
            button(oc=reset, "Reset"),
        ),
    )
```
