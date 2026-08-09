<!-- exp=baseline-haiku task=str_title_runs cond=psc sample=3 model=claude-haiku-4-5-20251001 cost_usd=0.011840700000000003 -->
```python
s = "it's a test and don't stop"
result = []
new_run = True

for char in s:
    if char.isalpha():
        if new_run:
            result.append(char.upper())
            new_run = False
        else:
            result.append(char.lower())
    else:
        result.append(char)
        new_run = True

print("".join(result))
```
