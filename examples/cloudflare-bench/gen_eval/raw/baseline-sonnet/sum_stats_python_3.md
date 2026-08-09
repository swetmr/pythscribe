<!-- exp=baseline-sonnet task=sum_stats cond=python sample=3 model=claude-sonnet-5 cost_usd=0.0094194 -->
```python
nums = [3, 7, 1, 9, 4]
total = sum(nums)
minimum = min(nums)
maximum = max(nums)
mean = total / len(nums)
print(f"{total} {minimum} {maximum} {mean:.2f}")
```
