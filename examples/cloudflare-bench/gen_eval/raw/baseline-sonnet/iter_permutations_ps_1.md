<!-- exp=baseline-sonnet task=iter_permutations cond=ps sample=1 model=claude-sonnet-5 cost_usd=0.027812999999999997 -->
```python
from itertools import permutations

perms = list(permutations([1, 2, 3]))
print(len(perms))
print(perms[:2])
```
