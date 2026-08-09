<!-- exp=baseline-haiku task=counter_most_common cond=psc sample=2 model=claude-haiku-4-5-20251001 cost_usd=0.0103049 -->
```python
from collections import Counter
word = "mississippi"
counts = Counter(word)
print(counts.most_common(3))
```
