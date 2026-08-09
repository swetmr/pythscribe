<!-- exp=baseline-sonnet task=iter_permutations cond=python sample=1 model=claude-sonnet-5 cost_usd=0.010326 -->
```python
from itertools import permutations

perms = list(permutations([1, 2, 3]))
print(len(perms))
print(perms[:2])
```
