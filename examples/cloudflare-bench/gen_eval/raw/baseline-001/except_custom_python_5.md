<!-- exp=baseline-001 task=except_custom cond=python sample=5 model=claude-opus-4-8 cost_usd=0.010719 -->
```python
class TooSmallError(Exception):
    pass


def check(n):
    if n < 10:
        raise TooSmallError("got 3")


try:
    check(3)
except TooSmallError as e:
    print("error: " + str(e))
```
