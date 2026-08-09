<!-- exp=baseline-haiku task=iter_combinations cond=python sample=4 model=claude-haiku-4-5-20251001 cost_usd=0.0044558 -->
```python
from itertools import combinations

s = "abcd"
for combo in combinations(s, 2):
    print(combo)
```
