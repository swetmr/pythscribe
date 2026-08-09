<!-- exp=baseline-sonnet task=str_title_runs cond=python sample=3 model=claude-sonnet-5 cost_usd=0.009510600000000001 -->
```python
import re

s = "it's a test and don't stop"
print(re.sub(r'[a-zA-Z]+', lambda m: m.group(0).capitalize(), s))
```
