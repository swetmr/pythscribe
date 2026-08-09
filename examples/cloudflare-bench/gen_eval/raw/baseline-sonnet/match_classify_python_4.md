<!-- exp=baseline-sonnet task=match_classify cond=python sample=4 model=claude-sonnet-5 cost_usd=0.0097731 -->
```python
def classify(value):
    match value:
        case 0:
            return "zero"
        case [x, y]:
            return f"pair {x},{y}"
        case _:
            return "other"


print(classify(0), classify([1, 2]), classify(7))
```
