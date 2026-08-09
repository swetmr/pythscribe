<!-- exp=baseline-haiku task=counter_repr cond=python sample=4 model=claude-haiku-4-5-20251001 cost_usd=0.008268600000000001 -->
```python
from collections import Counter

data = [1, 1, 2, 2, 2, 3]
counter = Counter(data)

# Sort by count descending
sorted_items = sorted(counter.items(), key=lambda x: x[1], reverse=True)
ordered_dict = dict(sorted_items)

print(f"Counter({ordered_dict})")
```
