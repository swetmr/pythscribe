<!-- exp=baseline-sonnet task=gen_fib cond=psc sample=2 model=claude-sonnet-5 cost_usd=0.009720300000000001 -->
```python
def fib():
    a, b = 0, 1
    while True:
        yield a
        a, b = b, a + b

g = fib()
print([next(g) for _ in range(10)])
```
