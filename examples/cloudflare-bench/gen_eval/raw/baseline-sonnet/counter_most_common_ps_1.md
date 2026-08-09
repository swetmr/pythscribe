<!-- exp=baseline-sonnet task=counter_most_common cond=ps sample=1 model=claude-sonnet-5 cost_usd=0.028149 -->
```python
from collections import Counter

word = "mississippi"
counts = Counter(word)
top3 = counts.most_common(3)
print(top3)
```
