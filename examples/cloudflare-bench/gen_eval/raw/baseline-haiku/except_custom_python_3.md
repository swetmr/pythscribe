<!-- exp=baseline-haiku task=except_custom cond=python sample=3 model=claude-haiku-4-5-20251001 cost_usd=0.007208600000000001 -->
```python
class TooSmallError(Exception):
    pass

def check_value(n):
    if n < 10:
        raise TooSmallError("got 3")

try:
    check_value(3)
except TooSmallError as e:
    print("error: " + str(e))
```
