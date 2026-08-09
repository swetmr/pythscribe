<!-- exp=baseline-haiku task=closure_adder cond=python sample=4 model=claude-haiku-4-5-20251001 cost_usd=0.0050391 -->
```python
def make_adder(n):
    def adder(x):
        return n + x
    return adder

add5 = make_adder(5)
add_neg2 = make_adder(-2)

print(add5(37), add_neg2(10))
```
