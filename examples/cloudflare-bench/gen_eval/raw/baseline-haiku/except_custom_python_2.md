<!-- exp=baseline-haiku task=except_custom cond=python sample=2 model=claude-haiku-4-5-20251001 cost_usd=0.0037286000000000003 -->
```python
class TooSmallError(Exception):
    pass

def check_value(n):
    if n < 10:
        raise TooSmallError(f"got {n}")

try:
    check_value(3)
except TooSmallError as e:
    print(f"error: {e}")
```
