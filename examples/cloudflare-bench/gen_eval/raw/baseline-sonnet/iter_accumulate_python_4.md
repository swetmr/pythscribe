<!-- exp=baseline-sonnet task=iter_accumulate cond=python sample=4 model=claude-sonnet-5 cost_usd=0.0087033 -->
```python
values = [3, 1, 4, 1, 5]
totals = []
running = 0
for v in values:
    running += v
    totals.append(running)
print(totals)
```
