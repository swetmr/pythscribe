<!-- exp=baseline-haiku task=counter_repr cond=python sample=5 model=claude-haiku-4-5-20251001 cost_usd=0.0056436 -->
```python
from collections import Counter

data = [1, 1, 2, 2, 2, 3]
counter = Counter(data)
ordered = Counter(counter.most_common())
print(ordered)
```
