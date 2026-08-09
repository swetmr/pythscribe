<!-- exp=baseline-sonnet task=iter_accumulate cond=ps sample=1 model=claude-sonnet-5 cost_usd=0.027846000000000003 -->
```python
nums = [3, 1, 4, 1, 5]
totals = []
running = 0
for n in nums:
    running += n
    totals.append(running)
print(totals)
```
