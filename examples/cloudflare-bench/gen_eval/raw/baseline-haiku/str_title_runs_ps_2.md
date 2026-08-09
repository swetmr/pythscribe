<!-- exp=baseline-haiku task=str_title_runs cond=ps sample=2 model=claude-haiku-4-5-20251001 cost_usd=0.0176215 -->
```python
s = "it's a test and don't stop"
result = []
start_of_run = True

for char in s:
    if char.isalpha():
        if start_of_run:
            result.append(char.upper())
            start_of_run = False
        else:
            result.append(char.lower())
    else:
        result.append(char)
        start_of_run = True

print(''.join(result))
```
