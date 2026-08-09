<!-- exp=baseline-001 task=gen_fib cond=python sample=5 model=claude-opus-4-8 cost_usd=0.0114575 -->
```python
def fibonacci():
    a, b = 0, 1
    while True:
        yield a
        a, b = b, a + b


def main():
    gen = fibonacci()
    values = [next(gen) for _ in range(10)]
    print(values)


if __name__ == "__main__":
    main()
```
