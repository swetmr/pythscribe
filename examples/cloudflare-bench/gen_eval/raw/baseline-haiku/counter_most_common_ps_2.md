<!-- exp=baseline-haiku task=counter_most_common cond=ps sample=2 model=claude-haiku-4-5-20251001 cost_usd=0.007075700000000001 -->
```python
from collections import Counter

word = "mississippi"
letter_counts = Counter(word)

first_appearance = {}
for i, letter in enumerate(word):
    if letter not in first_appearance:
        first_appearance[letter] = i

sorted_counts = sorted(letter_counts.items(), key=lambda x: (-x[1], first_appearance[x[0]]))
top_3 = sorted_counts[:3]

print(top_3)
```
