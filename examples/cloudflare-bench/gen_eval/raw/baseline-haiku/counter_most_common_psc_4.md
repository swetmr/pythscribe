<!-- exp=baseline-haiku task=counter_most_common cond=psc sample=4 model=claude-haiku-4-5-20251001 cost_usd=0.007169900000000001 -->
```python
from collections import Counter

word = "mississippi"
counts = Counter(word)
first_appearance = {}
for i, char in enumerate(word):
    if char not in first_appearance:
        first_appearance[char] = i

sorted_counts = sorted(counts.items(), key=lambda x: (-x[1], first_appearance[x[0]]))
top_3 = sorted_counts[:3]

print(top_3)
```
