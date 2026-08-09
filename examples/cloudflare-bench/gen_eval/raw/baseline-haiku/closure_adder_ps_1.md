<!-- exp=baseline-haiku task=closure_adder cond=ps sample=1 model=claude-haiku-4-5-20251001 cost_usd=0.010828299999999999 -->
```python
def make_adder(n):
    def adder(x):
        return x + n
    return adder

add5 = make_adder(5)
add_minus2 = make_adder(-2)

print(add5(37), add_minus2(10))
```
