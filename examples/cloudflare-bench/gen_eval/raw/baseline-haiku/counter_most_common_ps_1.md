<!-- exp=baseline-haiku task=counter_most_common cond=ps sample=1 model=claude-haiku-4-5-20251001 cost_usd=0.0157283 -->
```python
from collections import Counter

word = "mississippi"
counts = Counter(word)

# Track first appearance
first_appearance = {}
for i, letter in enumerate(word):
    if letter not in first_appearance:
        first_appearance[letter] = i

# Get the 3 most common, sorted by count (desc), then by first appearance
sorted_letters = sorted(counts.items(), key=lambda x: (-x[1], first_appearance[x[0]]))[:3]
print(sorted_letters)
```
