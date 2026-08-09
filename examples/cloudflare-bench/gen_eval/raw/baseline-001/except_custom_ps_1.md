<!-- exp=baseline-001 task=except_custom cond=ps sample=1 model=claude-opus-4-8 cost_usd=0.042065500000000006 -->
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
