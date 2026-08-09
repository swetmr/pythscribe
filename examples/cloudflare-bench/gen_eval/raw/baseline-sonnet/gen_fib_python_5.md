<!-- exp=baseline-sonnet task=gen_fib cond=python sample=5 model=claude-sonnet-5 cost_usd=0.0085272 -->
```python
def fibonacci():
    a, b = 0, 1
    while True:
        yield a
        a, b = b, a + b


gen = fibonacci()
print([next(gen) for _ in range(10)])
```
