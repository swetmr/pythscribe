<!-- exp=baseline-haiku task=sum_stats cond=python sample=4 model=claude-haiku-4-5-20251001 cost_usd=0.0037229 -->
```python
numbers = [3, 7, 1, 9, 4]
total = sum(numbers)
min_val = min(numbers)
max_val = max(numbers)
mean = total / len(numbers)

print(f"{total:.2f} {min_val:.2f} {max_val:.2f} {mean:.2f}")
```
