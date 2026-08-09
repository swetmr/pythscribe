<!-- exp=baseline-sonnet task=gen_fib cond=python sample=1 model=claude-sonnet-5 cost_usd=0.011412 -->
```python
def fibonacci():
    a, b = 0, 1
    while True:
        yield a
        a, b = b, a + b


def main():
    gen = fibonacci()
    print([next(gen) for _ in range(10)])


if __name__ == "__main__":
    main()
```
