<!-- exp=baseline-sonnet task=counter_most_common cond=python sample=5 model=claude-sonnet-5 cost_usd=0.008366700000000001 -->
```python
from collections import Counter

counts = Counter("mississippi")
print(counts.most_common(3))
```
