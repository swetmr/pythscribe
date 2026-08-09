<!-- exp=baseline-001 task=counter_repr cond=python sample=2 model=claude-opus-4-8 cost_usd=0.0152195 -->
```python
from collections import Counter

c = Counter([1, 1, 2, 2, 2, 3])
ordered = ", ".join(f"{k}: {v}" for k, v in c.most_common())
print(f"Counter({{{ordered}}})")
```
