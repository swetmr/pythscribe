<!-- exp=baseline-sonnet task=counter_most_common cond=python sample=4 model=claude-sonnet-5 cost_usd=0.0090567 -->
```python
from collections import Counter

counts = Counter("mississippi")
result = sorted(counts.items(), key=lambda x: -x[1])[:3]
print(result)
```
