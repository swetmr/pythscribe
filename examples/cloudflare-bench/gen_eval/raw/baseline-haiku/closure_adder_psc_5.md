<!-- exp=baseline-haiku task=closure_adder cond=psc sample=5 model=claude-haiku-4-5-20251001 cost_usd=0.005481400000000001 -->
```python
def make_adder(n):
    def adder(x):
        return x + n
    return adder

print(make_adder(5)(37), make_adder(-2)(10))
```
