<!-- exp=baseline-haiku task=closure_adder cond=python sample=5 model=claude-haiku-4-5-20251001 cost_usd=0.0050191 -->
```python
def make_adder(n):
    def adder(x):
        return x + n
    return adder

add_5 = make_adder(5)
add_neg_2 = make_adder(-2)

print(add_5(37), add_neg_2(10))
```
