<!-- exp=baseline-001 task=str_title_runs cond=python sample=4 model=claude-opus-4-8 cost_usd=0.013096499999999999 -->
```python
import re
s = "it's a test and don't stop"
print(re.sub(r"[A-Za-z]+", lambda m: m.group(0).capitalize(), s))
```
