<!-- exp=baseline-sonnet task=match_classify cond=python sample=1 model=claude-sonnet-5 cost_usd=0.011955 -->
```python
def classify(value):
    match value:
        case 0:
            return "zero"
        case [x, y]:
            return f"pair {x},{y}"
        case _:
            return "other"


print(" ".join(classify(v) for v in (0, [1, 2], 7)))
```
