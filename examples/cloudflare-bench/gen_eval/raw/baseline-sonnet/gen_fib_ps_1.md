<!-- exp=baseline-sonnet task=gen_fib cond=ps sample=1 model=claude-sonnet-5 cost_usd=0.028029 -->
```python
def fib():
    a, b = 0, 1
    while True:
        yield a
        a, b = b, a + b

g = fib()
print([next(g) for _ in range(10)])
```
