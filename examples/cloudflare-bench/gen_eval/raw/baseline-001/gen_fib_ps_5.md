<!-- exp=baseline-001 task=gen_fib cond=ps sample=5 model=claude-opus-4-8 cost_usd=0.010981 -->
```python
def fib():
    a, b = 0, 1
    while True:
        yield a
        a, b = b, a + b


gen = fib()
values = [next(gen) for _ in range(10)]
print(values)
```
