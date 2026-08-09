<!-- exp=baseline-001 task=str_title_runs cond=python sample=1 model=claude-opus-4-8 cost_usd=0.0184705 -->
```python
s = "it's a test and don't stop"
out = []
new_run = True
for ch in s:
    if ch.isalpha():
        out.append(ch.upper() if new_run else ch.lower())
        new_run = False
    else:
        out.append(ch)
        new_run = True
print("".join(out))
```
