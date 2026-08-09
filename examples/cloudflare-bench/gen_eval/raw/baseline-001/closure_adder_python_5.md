<!-- exp=baseline-001 task=closure_adder cond=python sample=5 model=claude-opus-4-8 cost_usd=0.012220000000000002 -->
```python
def make_adder(n):
    def adder(x):
        return x + n
    return adder


def main():
    print(make_adder(5)(37), make_adder(-2)(10))


if __name__ == "__main__":
    main()
```
