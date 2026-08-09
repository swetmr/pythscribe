<!-- exp=baseline-haiku task=closure_adder cond=ps sample=3 model=claude-haiku-4-5-20251001 cost_usd=0.0033222 -->
```python
def make_adder(n):
    def adder(x):
        return x + n
    return adder

result1 = make_adder(5)(37)
result2 = make_adder(-2)(10)
print(result1, result2)
```
