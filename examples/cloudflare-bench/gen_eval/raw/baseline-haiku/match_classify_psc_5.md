<!-- exp=baseline-haiku task=match_classify cond=psc sample=5 model=claude-haiku-4-5-20251001 cost_usd=0.0090732 -->
```python
def classify(value):
    match value:
        case 0:
            return "zero"
        case [x, y]:
            return f"pair {x},{y}"
        case _:
            return "other"

result = [classify(0), classify([1, 2]), classify(7)]
print(" ".join(result))
```
