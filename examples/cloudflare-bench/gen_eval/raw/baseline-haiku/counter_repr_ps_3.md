<!-- exp=baseline-haiku task=counter_repr cond=ps sample=3 model=claude-haiku-4-5-20251001 cost_usd=0.007451700000000001 -->
```python
from collections import Counter

data = [1, 1, 2, 2, 2, 3]
counter = Counter(data)
sorted_counter = Counter(dict(counter.most_common()))
print(sorted_counter)
```
