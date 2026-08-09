<!-- exp=baseline-haiku task=iter_accumulate cond=ps sample=5 model=claude-haiku-4-5-20251001 cost_usd=0.0035938999999999997 -->
```python
from itertools import accumulate

numbers = [3, 1, 4, 1, 5]
running_totals = list(accumulate(numbers))
print(running_totals)
```
