<!-- exp=baseline-haiku task=gen_fib cond=python sample=4 model=claude-haiku-4-5-20251001 cost_usd=0.0065564 -->
```python
def fibonacci():
    a, b = 0, 1
    while True:
        yield a
        a, b = b, a + b

gen = fibonacci()
fib_list = [next(gen) for _ in range(10)]
print(fib_list)
```
