<!-- exp=baseline-haiku task=match_classify cond=ps sample=1 model=claude-haiku-4-5-20251001 cost_usd=0.010349299999999999 -->
```python
def classify(value):
    match value:
        case 0:
            return "zero"
        case [x, y]:
            return f"pair {x},{y}"
        case _:
            return "other"

print(f"{classify(0)} {classify([1, 2])} {classify(7)}")
```
