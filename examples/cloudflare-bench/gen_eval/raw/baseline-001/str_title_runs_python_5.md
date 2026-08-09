<!-- exp=baseline-001 task=str_title_runs cond=python sample=5 model=claude-opus-4-8 cost_usd=0.017246499999999998 -->
```python
import re

s = "it's a test and don't stop"
result = re.sub(r'[A-Za-z]+', lambda m: m.group(0).capitalize(), s)
print(result)
```
