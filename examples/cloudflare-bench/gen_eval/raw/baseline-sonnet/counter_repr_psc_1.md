<!-- exp=baseline-sonnet task=counter_repr cond=psc sample=1 model=claude-sonnet-5 cost_usd=0.037247999999999996 -->
```python
from collections import Counter

c = Counter([1, 1, 2, 2, 2, 3])
print(Counter(dict(sorted(c.items(), key=lambda x: -x[1]))))
```
