<!-- exp=baseline-001 task=gen_fib cond=psc sample=5 model=claude-opus-4-8 cost_usd=0.011521 -->
```python
def fib():
    a, b = 0, 1
    while True:
        yield a
        a, b = b, a + b

g = fib()
print([next(g) for _ in range(10)])
```
