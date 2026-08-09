<!-- exp=baseline-haiku task=closure_adder cond=ps sample=2 model=claude-haiku-4-5-20251001 cost_usd=0.0051272 -->
```python
def make_adder(n):
    def adder(x):
        return x + n
    return adder

print(make_adder(5)(37), make_adder(-2)(10))
```
