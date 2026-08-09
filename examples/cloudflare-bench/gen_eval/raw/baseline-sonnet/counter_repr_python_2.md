<!-- exp=baseline-sonnet task=counter_repr cond=python sample=2 model=claude-sonnet-5 cost_usd=0.009389400000000001 -->
```python
from collections import Counter

data = [1, 1, 2, 2, 2, 3]
counter = Counter(data)
print(Counter(dict(counter.most_common())))
```
