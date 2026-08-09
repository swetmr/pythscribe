<!-- exp=baseline-001 task=match_classify cond=psc sample=5 model=claude-opus-4-8 cost_usd=0.0142475 -->
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
