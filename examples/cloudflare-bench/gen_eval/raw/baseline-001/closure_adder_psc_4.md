<!-- exp=baseline-001 task=closure_adder cond=psc sample=4 model=claude-opus-4-8 cost_usd=0.0111585 -->
```python
def make_adder(n):
    return lambda x: x + n

print(make_adder(5)(37), make_adder(-2)(10))
```
