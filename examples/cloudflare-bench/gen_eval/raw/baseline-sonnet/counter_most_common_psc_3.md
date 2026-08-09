<!-- exp=baseline-sonnet task=counter_most_common cond=psc sample=3 model=claude-sonnet-5 cost_usd=0.009664800000000001 -->
```python
from collections import Counter

counts = Counter("mississippi")
print(counts.most_common(3))
```
