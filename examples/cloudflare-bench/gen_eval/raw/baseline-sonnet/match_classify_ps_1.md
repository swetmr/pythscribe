<!-- exp=baseline-sonnet task=match_classify cond=ps sample=1 model=claude-sonnet-5 cost_usd=0.030807 -->
```python
def classify(v):
    match v:
        case 0:
            return "zero"
        case [x, y]:
            return f"pair {x},{y}"
        case _:
            return "other"

print(f"{classify(0)} {classify([1, 2])} {classify(7)}")
```
