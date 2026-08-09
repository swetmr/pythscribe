<!-- exp=baseline-haiku task=counter_most_common cond=ps sample=5 model=claude-haiku-4-5-20251001 cost_usd=0.007245700000000001 -->
```python
from collections import Counter

text = "mississippi"
counter = Counter(text)
most_common_3 = counter.most_common(3)
print(most_common_3)
```
