<!-- exp=baseline-sonnet task=closure_adder cond=python sample=2 model=claude-sonnet-5 cost_usd=0.0083097 -->
```python
def make_adder(n):
    def adder(x):
        return x + n
    return adder

print(make_adder(5)(37), make_adder(-2)(10))
```
