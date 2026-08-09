<!-- exp=baseline-haiku task=iter_accumulate cond=python sample=5 model=claude-haiku-4-5-20251001 cost_usd=0.0070908 -->
```python
from itertools import accumulate
nums = [3, 1, 4, 1, 5]
result = list(accumulate(nums))
print(result)
```
