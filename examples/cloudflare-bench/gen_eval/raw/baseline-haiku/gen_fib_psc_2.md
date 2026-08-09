<!-- exp=baseline-haiku task=gen_fib cond=psc sample=2 model=claude-haiku-4-5-20251001 cost_usd=0.0042337 -->
```python
from itertools import islice

def fibonacci():
    a, b = 0, 1
    while True:
        yield a
        a, b = b, a + b

print(list(islice(fibonacci(), 10)))
```
