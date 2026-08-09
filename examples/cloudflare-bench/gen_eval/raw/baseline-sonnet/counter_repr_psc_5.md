<!-- exp=baseline-sonnet task=counter_repr cond=psc sample=5 model=claude-sonnet-5 cost_usd=0.0146625 -->
```python
from collections import Counter

counts = Counter([1, 1, 2, 2, 2, 3])
print(Counter(dict(counts.most_common())))
```
