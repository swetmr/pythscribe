<!-- exp=baseline-sonnet task=counter_most_common cond=psc sample=5 model=claude-sonnet-5 cost_usd=0.0105048 -->
```python
from collections import Counter
c = Counter("mississippi")
print(c.most_common(3))
```
