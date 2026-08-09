<!-- exp=baseline-sonnet task=counter_most_common cond=python sample=2 model=claude-sonnet-5 cost_usd=0.0089967 -->
```python
from collections import Counter

counts = Counter("mississippi")
result = counts.most_common(3)
print(result)
```
