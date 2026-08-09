<!-- exp=baseline-haiku task=iter_permutations cond=ps sample=3 model=claude-haiku-4-5-20251001 cost_usd=0.0035650000000000005 -->
```python
from itertools import permutations

perms = list(permutations([1, 2, 3]))
print(len(perms))
print(perms[:2])
```
