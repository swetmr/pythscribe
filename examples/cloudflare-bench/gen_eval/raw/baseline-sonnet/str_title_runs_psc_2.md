<!-- exp=baseline-sonnet task=str_title_runs cond=psc sample=2 model=claude-sonnet-5 cost_usd=0.012788699999999998 -->
```python
import re

s = "it's a test and don't stop"
result = re.sub(r'[a-zA-Z]+', lambda m: m.group(0)[0].upper() + m.group(0)[1:].lower(), s)
print(result)
```
