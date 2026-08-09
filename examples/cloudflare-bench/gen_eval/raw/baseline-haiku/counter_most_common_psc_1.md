<!-- exp=baseline-haiku task=counter_most_common cond=psc sample=1 model=claude-haiku-4-5-20251001 cost_usd=0.0190773 -->
```python
from collections import Counter

word = "mississippi"
counts = Counter(word)
first_appearance = {char: word.index(char) for char in word}
sorted_counts = sorted(counts.items(), key=lambda x: (-x[1], first_appearance[x[0]]))
print(sorted_counts[:3])
```
