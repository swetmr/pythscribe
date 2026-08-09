<!-- exp=baseline-haiku task=counter_most_common cond=psc sample=3 model=claude-haiku-4-5-20251001 cost_usd=0.0066199 -->
```python
from collections import Counter

word = "mississippi"
counts = Counter(word)
result = counts.most_common(3)
print(result)
```
