<!-- exp=baseline-haiku task=counter_most_common cond=psc sample=5 model=claude-haiku-4-5-20251001 cost_usd=0.0109399 -->
```python
from collections import Counter

text = "mississippi"
counter = Counter(text)

first_appearance = {}
for i, letter in enumerate(text):
    if letter not in first_appearance:
        first_appearance[letter] = i

sorted_letters = sorted(counter.items(), key=lambda x: (-x[1], first_appearance[x[0]]))
top_3 = sorted_letters[:3]

print(top_3)
```
