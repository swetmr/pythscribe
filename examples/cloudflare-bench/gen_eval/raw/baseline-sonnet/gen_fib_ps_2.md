<!-- exp=baseline-sonnet task=gen_fib cond=ps sample=2 model=claude-sonnet-5 cost_usd=0.0093963 -->
```python
def fib():
    a, b = 0, 1
    while True:
        yield a
        a, b = b, a + b

gen = fib()
result = [next(gen) for _ in range(10)]
print(result)
```
