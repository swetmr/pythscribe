<!-- exp=baseline-001 task=str_title_runs cond=ps sample=1 model=claude-opus-4-8 cost_usd=0.0506905 -->
```python
s = "it's a test and don't stop"
result = ""
new_run = True
for ch in s:
    if ch.isalpha():
        result += ch.upper() if new_run else ch.lower()
        new_run = False
    else:
        result += ch
        new_run = True
print(result)
```
