<!-- exp=baseline-haiku task=counter_most_common cond=python sample=5 model=claude-haiku-4-5-20251001 cost_usd=0.0046976000000000006 -->
```python
from collections import Counter

word = "mississippi"
counts = Counter(word)
sorted_counts = sorted(counts.items(), key=lambda x: (-x[1], word.index(x[0])))[:3]
print(sorted_counts)
```
