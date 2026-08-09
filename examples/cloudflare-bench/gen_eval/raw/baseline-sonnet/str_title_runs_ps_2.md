<!-- exp=baseline-sonnet task=str_title_runs cond=ps sample=2 model=claude-sonnet-5 cost_usd=0.014384699999999999 -->
```python
s = "it's a test and don't stop"
result = []
prev_alpha = False
for ch in s:
    if ch.isalpha():
        if not prev_alpha:
            result.append(ch.upper())
        else:
            result.append(ch.lower())
        prev_alpha = True
    else:
        result.append(ch)
        prev_alpha = False
print("".join(result))
```
