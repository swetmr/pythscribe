<!-- exp=baseline-sonnet task=list_mutate cond=python sample=1 model=claude-sonnet-5 cost_usd=0.010725000000000002 -->
```python
lst = [1, 2]
lst.append(3)
lst.extend([4, 5])
lst.insert(0, 0)
removed = lst.pop()
print(removed)
print(lst)
```
