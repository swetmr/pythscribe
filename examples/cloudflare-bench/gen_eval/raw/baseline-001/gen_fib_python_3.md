<!-- exp=baseline-001 task=gen_fib cond=python sample=3 model=claude-opus-4-8 cost_usd=0.0111325 -->
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
