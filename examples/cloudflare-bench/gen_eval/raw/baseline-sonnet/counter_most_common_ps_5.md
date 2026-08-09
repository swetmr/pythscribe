<!-- exp=baseline-sonnet task=counter_most_common cond=ps sample=5 model=claude-sonnet-5 cost_usd=0.0103008 -->
```python
from collections import Counter

counts = Counter("mississippi")
print(counts.most_common(3))
```
