<!-- exp=baseline-001 task=counter_repr cond=ps sample=1 model=claude-opus-4-8 cost_usd=0.04605050000000001 -->
```python
from collections import Counter

data = [1, 1, 2, 2, 2, 3]
counts = Counter(data)
ordered = Counter(dict(counts.most_common()))
print(ordered)
```
