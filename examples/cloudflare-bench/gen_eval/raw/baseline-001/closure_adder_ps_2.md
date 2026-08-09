<!-- exp=baseline-001 task=closure_adder cond=ps sample=2 model=claude-opus-4-8 cost_usd=0.011093500000000001 -->
```python
def make_adder(n):
    def adder(x):
        return x + n
    return adder

print(make_adder(5)(37), make_adder(-2)(10))
```
