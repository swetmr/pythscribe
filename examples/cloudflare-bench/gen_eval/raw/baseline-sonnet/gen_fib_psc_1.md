<!-- exp=baseline-sonnet task=gen_fib cond=psc sample=1 model=claude-sonnet-5 cost_usd=0.037418999999999994 -->
```python
def fib_gen():
    a, b = 0, 1
    while True:
        yield a
        a, b = b, a + b

g = fib_gen()
print([next(g) for _ in range(10)])
```
