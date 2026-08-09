<!-- exp=baseline-sonnet task=closure_adder cond=ps sample=1 model=claude-sonnet-5 cost_usd=0.028029000000000005 -->
```python
def make_adder(n):
    def adder(x):
        return x + n
    return adder

add5 = make_adder(5)
add_neg2 = make_adder(-2)
print(f"{add5(37)} {add_neg2(10)}")
```
