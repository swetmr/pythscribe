<!-- exp=baseline-haiku task=sum_stats cond=ps sample=4 model=claude-haiku-4-5-20251001 cost_usd=0.006341000000000001 -->
```python
numbers = [3, 7, 1, 9, 4]
total = sum(numbers)
min_val = min(numbers)
max_val = max(numbers)
mean = total / len(numbers)

print(f"{total} {min_val} {max_val} {mean:.2f}")
```
