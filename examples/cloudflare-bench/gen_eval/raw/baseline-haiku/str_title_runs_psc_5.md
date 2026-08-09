<!-- exp=baseline-haiku task=str_title_runs cond=psc sample=5 model=claude-haiku-4-5-20251001 cost_usd=0.007525700000000001 -->
```python
s = "it's a test and don't stop"
result = []
in_run = False

for char in s:
    if char.isalpha():
        if not in_run:
            result.append(char.upper())
            in_run = True
        else:
            result.append(char.lower())
    else:
        result.append(char)
        in_run = False

print(''.join(result))
```
