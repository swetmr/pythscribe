<!-- exp=baseline-haiku task=list_mutate cond=psc sample=3 model=claude-haiku-4-5-20251001 cost_usd=0.0094058 -->
```python
xs = [1, 2]
xs.append(3)
xs.extend([4, 5])
xs.insert(0, 0)
removed = xs.pop()
print(removed)
print(xs)
```
