<!-- exp=baseline-sonnet task=str_title_runs cond=psc sample=4 model=claude-sonnet-5 cost_usd=0.0152937 -->
```python
import re
s = "it's a test and don't stop"
print(re.sub(r'[a-zA-Z]+', lambda m: m.group(0).capitalize(), s))
```
