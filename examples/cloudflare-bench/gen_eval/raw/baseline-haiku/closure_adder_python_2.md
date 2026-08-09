<!-- exp=baseline-haiku task=closure_adder cond=python sample=2 model=claude-haiku-4-5-20251001 cost_usd=0.0031691 -->
```python
def make_adder(n):
    def adder(x):
        return n + x
    return adder

print(make_adder(5)(37), make_adder(-2)(10))
```
