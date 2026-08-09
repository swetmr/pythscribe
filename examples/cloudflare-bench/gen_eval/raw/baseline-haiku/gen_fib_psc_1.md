<!-- exp=baseline-haiku task=gen_fib cond=psc sample=1 model=claude-haiku-4-5-20251001 cost_usd=0.0159433 -->
```python
from itertools import islice

def fib():
    a, b = 0, 1
    yield a
    while True:
        yield b
        a, b = b, a + b

print(list(islice(fib(), 10)))
```
