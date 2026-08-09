<!-- exp=baseline-haiku task=iter_combinations cond=ps sample=4 model=claude-haiku-4-5-20251001 cost_usd=0.0044639 -->
```python
from itertools import combinations

chars = "abcd"
for comb in combinations(chars, 2):
    print(comb)
```
