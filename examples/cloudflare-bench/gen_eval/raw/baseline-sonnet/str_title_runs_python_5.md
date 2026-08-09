<!-- exp=baseline-sonnet task=str_title_runs cond=python sample=5 model=claude-sonnet-5 cost_usd=0.0091056 -->
```python
import re

s = "it's a test and don't stop"
result = re.sub(r"[A-Za-z]+", lambda m: m.group(0)[0].upper() + m.group(0)[1:].lower(), s)
print(result)
```
