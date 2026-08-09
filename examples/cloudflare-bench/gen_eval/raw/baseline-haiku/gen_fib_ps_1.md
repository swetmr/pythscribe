<!-- exp=baseline-haiku task=gen_fib cond=ps sample=1 model=claude-haiku-4-5-20251001 cost_usd=0.012034300000000001 -->
```python
def fibonacci():
    a, b = 0, 1
    while True:
        yield a
        a, b = b, a + b

fib_gen = fibonacci()
fib_list = [next(fib_gen) for _ in range(10)]
print(fib_list)
```
