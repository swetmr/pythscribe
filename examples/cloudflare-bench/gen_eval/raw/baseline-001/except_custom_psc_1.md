<!-- exp=baseline-001 task=except_custom cond=psc sample=1 model=claude-opus-4-8 cost_usd=0.0568655 -->
```python
class TooSmallError(Exception):
    pass

def check(n):
    if n < 10:
        raise TooSmallError(f"got {n}")

try:
    check(3)
except TooSmallError as e:
    print(f"error: {e}")
```
