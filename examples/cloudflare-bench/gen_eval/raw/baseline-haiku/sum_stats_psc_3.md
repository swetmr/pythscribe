<!-- exp=baseline-haiku task=sum_stats cond=psc sample=3 model=claude-haiku-4-5-20251001 cost_usd=0.0048352 -->
```python
nums = [3, 7, 1, 9, 4]
s = sum(nums)
mn = min(nums)
mx = max(nums)
mean = sum(nums) / len(nums)
print(f"{s} {mn} {mx} {mean:.2f}")
```
