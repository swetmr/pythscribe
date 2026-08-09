<!-- exp=baseline-haiku task=except_custom cond=psc sample=1 model=claude-haiku-4-5-20251001 cost_usd=0.013127300000000001 -->
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
