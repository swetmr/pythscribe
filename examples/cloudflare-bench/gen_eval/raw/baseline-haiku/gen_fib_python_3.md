<!-- exp=baseline-haiku task=gen_fib cond=python sample=3 model=claude-haiku-4-5-20251001 cost_usd=0.0039564 -->
```python
def fibonacci():
    a, b = 0, 1
    while True:
        yield a
        a, b = b, a + b

fib_gen = fibonacci()
first_10 = [next(fib_gen) for _ in range(10)]
print(first_10)
```
