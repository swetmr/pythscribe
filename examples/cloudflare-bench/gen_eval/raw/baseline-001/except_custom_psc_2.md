<!-- exp=baseline-001 task=except_custom cond=psc sample=2 model=claude-opus-4-8 cost_usd=0.012082500000000001 -->
```python
class TooSmallError(Exception):
    pass

def check(n):
    if n < 10:
        raise TooSmallError(f"got {n}")

try:
    check(3)
except TooSmallError as e:
    print("error: " + str(e))
```
