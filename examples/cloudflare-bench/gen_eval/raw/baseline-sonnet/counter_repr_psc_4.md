<!-- exp=baseline-sonnet task=counter_repr cond=psc sample=4 model=claude-sonnet-5 cost_usd=0.0190425 -->
```python
from collections import Counter

data = [1, 1, 2, 2, 2, 3]
c = Counter(data)
s = ", ".join(f"{k}: {v}" for k, v in c.most_common())
print(f"Counter({{{s}}})")
```
