<!-- exp=baseline-001 task=except_custom cond=python sample=4 model=claude-opus-4-8 cost_usd=0.009819000000000001 -->
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
