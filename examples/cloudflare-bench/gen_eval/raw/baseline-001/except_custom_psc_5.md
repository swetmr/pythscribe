<!-- exp=baseline-001 task=except_custom cond=psc sample=5 model=claude-opus-4-8 cost_usd=0.0120325 -->
```python
class TooSmallError(Exception): pass

def check(x):
    if x < 10:
        raise TooSmallError(f"got {x}")

try:
    check(3)
except TooSmallError as e:
    print(f"error: {e}")
```
