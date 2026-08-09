<!-- exp=baseline-sonnet task=counter_most_common cond=ps sample=4 model=claude-sonnet-5 cost_usd=0.0097608 -->
```python
from collections import Counter

counts = Counter("mississippi")
top3 = counts.most_common(3)
print(top3)
```
