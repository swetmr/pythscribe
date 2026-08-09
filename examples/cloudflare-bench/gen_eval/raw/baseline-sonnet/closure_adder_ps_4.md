<!-- exp=baseline-sonnet task=closure_adder cond=ps sample=4 model=claude-sonnet-5 cost_usd=0.009523799999999999 -->
```python
def make_adder(n):
    def adder(x):
        return x + n
    return adder

add5 = make_adder(5)
sub2 = make_adder(-2)
print(f"{add5(37)} {sub2(10)}")
```
