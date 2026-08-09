<!-- exp=baseline-haiku task=counter_repr cond=psc sample=4 model=claude-haiku-4-5-20251001 cost_usd=0.0108409 -->
```python
from collections import Counter

data = [1, 1, 2, 2, 2, 3]
counter = Counter(data)
ordered = {k: v for k, v in counter.most_common()}
print(f"Counter({ordered})")
```
