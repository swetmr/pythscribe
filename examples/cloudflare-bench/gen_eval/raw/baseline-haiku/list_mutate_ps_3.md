<!-- exp=baseline-haiku task=list_mutate cond=ps sample=3 model=claude-haiku-4-5-20251001 cost_usd=0.0063166 -->
```python
nums = [1, 2]
nums.append(3)
nums.extend([4, 5])
nums.insert(0, 0)
removed = nums.pop()
print(removed)
print(nums)
```
