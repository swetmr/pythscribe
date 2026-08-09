<!-- exp=baseline-haiku task=gen_fib cond=python sample=5 model=claude-haiku-4-5-20251001 cost_usd=0.0088664 -->
```python
def fibonacci():
    a, b = 0, 1
    while True:
        yield a
        a, b = b, a + b

fib = fibonacci()
result = [next(fib) for _ in range(10)]
print(result)
```
