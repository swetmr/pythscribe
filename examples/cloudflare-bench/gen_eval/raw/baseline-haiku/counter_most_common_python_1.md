<!-- exp=baseline-haiku task=counter_most_common cond=python sample=1 model=claude-haiku-4-5-20251001 cost_usd=0.0119263 -->
```python
from collections import Counter

word = "mississippi"
letter_counts = Counter(word)

first_appearance = {}
for i, letter in enumerate(word):
    if letter not in first_appearance:
        first_appearance[letter] = i

sorted_letters = sorted(letter_counts.items(), key=lambda x: (-x[1], first_appearance[x[0]]))
top_3 = sorted_letters[:3]

print(top_3)
```
