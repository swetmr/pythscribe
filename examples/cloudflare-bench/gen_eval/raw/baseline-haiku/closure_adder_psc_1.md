<!-- exp=baseline-haiku task=closure_adder cond=psc sample=1 model=claude-haiku-4-5-20251001 cost_usd=0.0132673 -->
```python
def make_adder(n):
    return lambda x: x + n

print(make_adder(5)(37), make_adder(-2)(10))
```
