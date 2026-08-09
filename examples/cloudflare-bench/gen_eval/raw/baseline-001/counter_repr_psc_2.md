<!-- exp=baseline-001 task=counter_repr cond=psc sample=2 model=claude-opus-4-8 cost_usd=0.015683 -->
```python
from collections import Counter
c = Counter([1, 1, 2, 2, 2, 3])
ordered = ", ".join(f"{k}: {v}" for k, v in c.most_common())
print(f"Counter({{{ordered}}})")
```
