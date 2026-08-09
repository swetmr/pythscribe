<!-- exp=baseline-haiku task=counter_most_common cond=python sample=3 model=claude-haiku-4-5-20251001 cost_usd=0.0065826 -->
```python
from collections import Counter

text = "mississippi"
counts = Counter(text)

# Track first appearance
first_appearance = {}
for i, char in enumerate(text):
    if char not in first_appearance:
        first_appearance[char] = i

# Sort by count (descending), then by first appearance (ascending)
sorted_counts = sorted(counts.items(), key=lambda x: (-x[1], first_appearance[x[0]]))

# Get top 3
result = sorted_counts[:3]
print(result)
```
