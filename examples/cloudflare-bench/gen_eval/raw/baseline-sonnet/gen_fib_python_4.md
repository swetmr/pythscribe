<!-- exp=baseline-sonnet task=gen_fib cond=python sample=4 model=claude-sonnet-5 cost_usd=0.0087672 -->
```python
def fib():
    a, b = 0, 1
    while True:
        yield a
        a, b = b, a + b


def main():
    gen = fib()
    print([next(gen) for _ in range(10)])


if __name__ == "__main__":
    main()
```
