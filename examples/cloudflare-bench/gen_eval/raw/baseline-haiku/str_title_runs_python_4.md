<!-- exp=baseline-haiku task=str_title_runs cond=python sample=4 model=claude-haiku-4-5-20251001 cost_usd=0.0097484 -->
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
