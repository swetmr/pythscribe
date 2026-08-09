<!-- exp=baseline-001 task=match_classify cond=python sample=2 model=claude-opus-4-8 cost_usd=0.012284 -->
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
