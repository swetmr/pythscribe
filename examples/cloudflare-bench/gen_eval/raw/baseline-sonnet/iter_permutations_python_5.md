<!-- exp=baseline-sonnet task=iter_permutations cond=python sample=5 model=claude-sonnet-5 cost_usd=0.0105519 -->
```python
from itertools import permutations

perms = list(permutations([1, 2, 3]))
print(len(perms))
print(perms[:2])
```
