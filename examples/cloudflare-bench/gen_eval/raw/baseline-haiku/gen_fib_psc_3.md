<!-- exp=baseline-haiku task=gen_fib cond=psc sample=3 model=claude-haiku-4-5-20251001 cost_usd=0.010548700000000001 -->
```python
def fib():
    a, b = 0, 1
    while True:
        yield a
        a, b = b, a + b

gen = fib()
print([next(gen) for _ in range(10)])
```
