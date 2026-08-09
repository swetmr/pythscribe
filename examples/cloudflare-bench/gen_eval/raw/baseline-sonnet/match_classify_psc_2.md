<!-- exp=baseline-sonnet task=match_classify cond=psc sample=2 model=claude-sonnet-5 cost_usd=0.0100662 -->
```python
def classify(v):
    match v:
        case 0:
            return "zero"
        case [x, y]:
            return f"pair {x},{y}"
        case _:
            return "other"

print(classify(0), classify([1, 2]), classify(7))
```
