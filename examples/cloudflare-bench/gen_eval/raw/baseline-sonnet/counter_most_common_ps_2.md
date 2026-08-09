<!-- exp=baseline-sonnet task=counter_most_common cond=ps sample=2 model=claude-sonnet-5 cost_usd=0.0092958 -->
This is a simple, self-contained task with no ambiguity — no skill needed here.

```python
from collections import Counter

counts = Counter("mississippi")
print(counts.most_common(3))
```
