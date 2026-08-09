<!-- exp=baseline-001 task=gen_fib cond=python sample=2 model=claude-opus-4-8 cost_usd=0.0114575 -->
```python
def fib():
    a, b = 0, 1
    while True:
        yield a
        a, b = b, a + b


def first_n(n):
    gen = fib()
    return [next(gen) for _ in range(n)]


print(first_n(10))
```
