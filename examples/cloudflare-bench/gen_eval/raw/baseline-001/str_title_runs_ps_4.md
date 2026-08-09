<!-- exp=baseline-001 task=str_title_runs cond=ps sample=4 model=claude-opus-4-8 cost_usd=0.01777 -->
```python
s = "it's a test and don't stop"
result = []
new_run = True
for ch in s:
    if ch.isalpha():
        if new_run:
            result.append(ch.upper())
        else:
            result.append(ch.lower())
        new_run = False
    else:
        result.append(ch)
        new_run = True
print("".join(result))
```
