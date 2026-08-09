<!-- exp=baseline-haiku task=list_mutate cond=ps sample=4 model=claude-haiku-4-5-20251001 cost_usd=0.0040616 -->
```python
lst = [1, 2]
lst.append(3)
lst.extend([4, 5])
lst.insert(0, 0)
removed = lst.pop()
print(removed)
print(lst)
```
