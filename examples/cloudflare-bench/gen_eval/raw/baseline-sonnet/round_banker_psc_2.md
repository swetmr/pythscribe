<!-- exp=baseline-sonnet task=round_banker cond=psc sample=2 model=claude-sonnet-5 cost_usd=0.0107154 -->
```python
def r(x):
    f = math.floor(x)
    d = x - f
    if d < 0.5:
        return f
    if d > 0.5:
        return f + 1
    return f if f % 2 == 0 else f + 1

import math
print(r(0.5), r(1.5), r(2.5), r(-0.5))
```
