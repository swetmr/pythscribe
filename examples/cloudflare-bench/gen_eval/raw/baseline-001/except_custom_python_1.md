<!-- exp=baseline-001 task=except_custom cond=python sample=1 model=claude-opus-4-8 cost_usd=0.0130705 -->
```python
class TooSmallError(Exception):
    pass


def check(value):
    if value < 10:
        raise TooSmallError("got 3")


try:
    check(3)
except TooSmallError as exc:
    print("error: " + str(exc))
```
