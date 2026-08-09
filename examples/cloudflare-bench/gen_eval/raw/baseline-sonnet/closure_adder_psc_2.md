<!-- exp=baseline-sonnet task=closure_adder cond=psc sample=2 model=claude-sonnet-5 cost_usd=0.009502799999999999 -->
```python
def make_adder(n):
    return lambda x: x + n

print(make_adder(5)(37), make_adder(-2)(10))
```
