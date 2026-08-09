<!-- exp=baseline-sonnet task=counter_repr cond=psc sample=3 model=claude-sonnet-5 cost_usd=0.020617499999999997 -->
```python
from collections import Counter

c = Counter([1, 1, 2, 2, 2, 3])
print(Counter(dict(c.most_common())))
```
