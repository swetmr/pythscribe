<!-- exp=baseline-haiku task=match_classify cond=ps sample=4 model=claude-haiku-4-5-20251001 cost_usd=0.0036439999999999997 -->
```python
def classify(value):
    match value:
        case 0:
            return "zero"
        case [x, y]:
            return f"pair {x},{y}"
        case _:
            return "other"

result1 = classify(0)
result2 = classify([1, 2])
result3 = classify(7)

print(f"{result1} {result2} {result3}")
```
