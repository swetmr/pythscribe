<!-- exp=baseline-sonnet task=gen_fib cond=ps sample=5 model=claude-sonnet-5 cost_usd=0.0101763 -->
```python
def fib_gen():
    a, b = 0, 1
    while True:
        yield a
        a, b = b, a + b

gen = fib_gen()
result = [next(gen) for _ in range(10)]
print(result)
```
