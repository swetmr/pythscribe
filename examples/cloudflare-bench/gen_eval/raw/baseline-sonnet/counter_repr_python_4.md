<!-- exp=baseline-sonnet task=counter_repr cond=python sample=4 model=claude-sonnet-5 cost_usd=0.0093594 -->
```python
from collections import Counter

c = Counter([1, 1, 2, 2, 2, 3])
print(Counter(dict(c.most_common())))
```
