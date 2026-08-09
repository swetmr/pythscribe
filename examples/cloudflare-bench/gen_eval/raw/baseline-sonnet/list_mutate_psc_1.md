<!-- exp=baseline-sonnet task=list_mutate cond=psc sample=1 model=claude-sonnet-5 cost_usd=0.038336999999999996 -->
```python
xs = [1, 2]
xs.append(3)
xs.extend([4, 5])
xs.insert(0, 0)
removed = xs.pop()
print(removed)
print(xs)
```
