<!-- exp=baseline-sonnet task=iter_accumulate cond=python sample=5 model=claude-sonnet-5 cost_usd=0.0086883 -->
```python
data = [3, 1, 4, 1, 5]
totals = []
running = 0
for n in data:
    running += n
    totals.append(running)
print(totals)
```
