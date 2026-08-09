<!-- exp=baseline-haiku task=closure_adder cond=psc sample=3 model=claude-haiku-4-5-20251001 cost_usd=0.0048814 -->
```python
def make_adder(n):
    return lambda x: x + n

print(f"{make_adder(5)(37)} {make_adder(-2)(10)}")
```
