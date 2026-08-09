<!-- exp=baseline-haiku task=counter_most_common cond=ps sample=3 model=claude-haiku-4-5-20251001 cost_usd=0.006600700000000001 -->
```python
from collections import Counter

word = "mississippi"
letter_counts = Counter(word)
first_appearance = {letter: word.index(letter) for letter in set(word)}
sorted_letters = sorted(letter_counts.items(), 
                       key=lambda x: (-x[1], first_appearance[x[0]]))
print(sorted_letters[:3])
```
