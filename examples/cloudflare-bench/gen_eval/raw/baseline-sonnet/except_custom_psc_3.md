<!-- exp=baseline-sonnet task=except_custom cond=psc sample=3 model=claude-sonnet-5 cost_usd=0.0117972 -->
```python
class TooSmallError(Exception):
    pass

def check(n):
    if n < 10:
        raise TooSmallError("got 3")

try:
    check(3)
except TooSmallError as e:
    print(f"error: {e}")
```
