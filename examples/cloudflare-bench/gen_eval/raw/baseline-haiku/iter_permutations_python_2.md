<!-- exp=baseline-haiku task=iter_permutations cond=python sample=2 model=claude-haiku-4-5-20251001 cost_usd=0.0027619 -->
```python
from itertools import permutations

perms = list(permutations([1, 2, 3]))
print(len(perms))
print(perms[:2])
```
