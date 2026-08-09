<!-- exp=baseline-001 task=str_title_runs cond=ps sample=5 model=claude-opus-4-8 cost_usd=0.021070000000000002 -->
```python
s = "it's a test and don't stop"
result = []
prev_alpha = False
for ch in s:
    if ch.isalpha():
        result.append(ch.upper() if not prev_alpha else ch.lower())
        prev_alpha = True
    else:
        result.append(ch)
        prev_alpha = False
print(''.join(result))
```
