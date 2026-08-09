<!-- exp=baseline-sonnet task=str_title_runs cond=ps sample=1 model=claude-sonnet-5 cost_usd=0.034512 -->
```python
import re

s = "it's a test and don't stop"
result = re.sub(r"[A-Za-z]+", lambda m: m.group(0).capitalize(), s)
print(result)
```
