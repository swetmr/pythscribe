<!-- exp=baseline-sonnet task=list_mutate cond=psc sample=3 model=claude-sonnet-5 cost_usd=0.0106407 -->
```python
xs = [1, 2]
xs.append(3)
xs.extend([4, 5])
xs.insert(0, 0)
removed = xs.pop()
print(removed)
print(xs)
```
