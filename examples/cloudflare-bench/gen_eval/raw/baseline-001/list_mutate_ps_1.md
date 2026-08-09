<!-- exp=baseline-001 task=list_mutate cond=ps sample=1 model=claude-opus-4-8 cost_usd=0.0419905 -->
```python
xs = [1, 2]
xs.append(3)
xs.extend([4, 5])
xs.insert(0, 0)
removed = xs.pop()
print(removed)
print(xs)
```
