<!-- exp=baseline-sonnet task=except_custom cond=python sample=4 model=claude-sonnet-5 cost_usd=0.009644100000000001 -->
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
