<!-- exp=baseline-haiku task=str_title_runs cond=psc sample=2 model=claude-haiku-4-5-20251001 cost_usd=0.0171693 -->
```python
s = "it's a test and don't stop"
result = []
new_run = True
for c in s:
    if c.isalpha():
        result.append(c.upper() if new_run else c.lower())
        new_run = False
    else:
        result.append(c)
        new_run = True
print(''.join(result))
```
