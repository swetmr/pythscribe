<!-- exp=baseline-haiku task=gen_fib cond=ps sample=5 model=claude-haiku-4-5-20251001 cost_usd=0.0146795 -->
```python
def fib():
    a, b = 0, 1
    while True:
        yield a
        a, b = b, a + b

gen = fib()
fibs = [next(gen) for _ in range(10)]
print(fibs)
```
