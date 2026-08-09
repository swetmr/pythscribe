<!-- exp=baseline-haiku task=counter_repr cond=python sample=2 model=claude-haiku-4-5-20251001 cost_usd=0.007588600000000001 -->
```python
from collections import Counter

data = [1, 1, 2, 2, 2, 3]
counter = Counter(data)
ordered = Counter(sorted(counter.items(), key=lambda x: -x[1]))
print(ordered)
```
