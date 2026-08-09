<!-- exp=baseline-haiku task=except_custom cond=ps sample=1 model=claude-haiku-4-5-20251001 cost_usd=0.0110283 -->
```python
class TooSmallError(Exception):
    pass

def check_value(x):
    if x < 10:
        raise TooSmallError("got 3")

try:
    check_value(3)
except TooSmallError as e:
    print(f"error: {e}")
```
