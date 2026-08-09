<!-- exp=baseline-001 task=str_title_runs cond=psc sample=1 model=claude-opus-4-8 cost_usd=0.06106550000000001 -->
```python
s = "it's a test and don't stop"
out = []
new = True
for ch in s:
    if ch.isalpha():
        out.append(ch.upper() if new else ch.lower())
        new = False
    else:
        out.append(ch)
        new = True
print("".join(out))
```
