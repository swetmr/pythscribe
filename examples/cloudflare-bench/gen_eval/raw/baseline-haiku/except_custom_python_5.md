<!-- exp=baseline-haiku task=except_custom cond=python sample=5 model=claude-haiku-4-5-20251001 cost_usd=0.0046236 -->
```python
class TooSmallError(Exception):
    pass

def check_value(x):
    if x < 10:
        raise TooSmallError(f"got {x}")

try:
    check_value(3)
except TooSmallError as e:
    print(f"error: {e}")
```
