<!-- exp=baseline-haiku task=set_ops cond=ps sample=2 model=claude-haiku-4-5-20251001 cost_usd=0.0066825 -->
```python
set1 = {1, 2, 3, 4, 5}
set2 = {4, 5, 6, 7, 8}

union_result = sorted(list(set1 | set2))
intersection_result = sorted(list(set1 & set2))
difference_result = sorted(list(set1 - set2))

print(union_result)
print(intersection_result)
print(difference_result)
```
