<!-- exp=baseline-sonnet task=counter_repr cond=python sample=5 model=claude-sonnet-5 cost_usd=0.0095094 -->
```python
from collections import Counter

data = [1, 1, 2, 2, 2, 3]
counter = Counter(data)
print(Counter(dict(counter.most_common())))
```
