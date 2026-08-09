<!-- exp=baseline-001 task=str_title_runs cond=psc sample=5 model=claude-opus-4-8 cost_usd=0.016485 -->
```python
s = "it's a test and don't stop"
out = []
prev_alpha = False
for ch in s:
    if ch.isalpha():
        out.append(ch.upper() if not prev_alpha else ch.lower())
        prev_alpha = True
    else:
        out.append(ch)
        prev_alpha = False
print("".join(out))
```
