<!-- exp=baseline-sonnet task=iter_accumulate cond=python sample=3 model=claude-sonnet-5 cost_usd=0.0087033 -->
```python
data = [3, 1, 4, 1, 5]
totals = []
running = 0
for n in data:
    running += n
    totals.append(running)
print(totals)
```
