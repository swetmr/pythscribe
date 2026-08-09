<!-- exp=baseline-sonnet task=iter_accumulate cond=python sample=2 model=claude-sonnet-5 cost_usd=0.0089133 -->
```python
nums = [3, 1, 4, 1, 5]
total = 0
totals = []
for n in nums:
    total += n
    totals.append(total)
print(totals)
```
