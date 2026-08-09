<!-- exp=baseline-sonnet task=str_title_runs cond=python sample=2 model=claude-sonnet-5 cost_usd=0.0090306 -->
```python
import re

s = "it's a test and don't stop"
result = re.sub(r'[A-Za-z]+', lambda m: m.group(0).capitalize(), s)
print(result)
```
