<!-- exp=baseline-sonnet task=gen_fib cond=python sample=2 model=claude-sonnet-5 cost_usd=0.0087372 -->
```python
def fibonacci():
    a, b = 0, 1
    while True:
        yield a
        a, b = b, a + b


if __name__ == "__main__":
    gen = fibonacci()
    print([next(gen) for _ in range(10)])
```
