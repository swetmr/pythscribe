<!-- exp=baseline-haiku task=counter_repr cond=python sample=1 model=claude-haiku-4-5-20251001 cost_usd=0.0093613 -->
```python
from collections import Counter

data = [1, 1, 2, 2, 2, 3]
counter = Counter(data)
most_common = counter.most_common()
formatted = "Counter({" + ", ".join(f"{k}: {v}" for k, v in most_common) + "})"
print(formatted)
```
