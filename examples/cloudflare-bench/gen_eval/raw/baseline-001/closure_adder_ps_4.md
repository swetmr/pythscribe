<!-- exp=baseline-001 task=closure_adder cond=ps sample=4 model=claude-opus-4-8 cost_usd=0.0115435 -->
```python
def make_adder(n):
    return lambda x: x + n

add5 = make_adder(5)
add_neg2 = make_adder(-2)
print(f"{add5(37)} {add_neg2(10)}")
```
