<!-- exp=baseline-haiku task=str_title_runs cond=ps sample=3 model=claude-haiku-4-5-20251001 cost_usd=0.0159565 -->
```python
text = "it's a test and don't stop"
result = []
start_of_run = True

for char in text:
    if char.isalpha():
        result.append(char.upper() if start_of_run else char.lower())
        start_of_run = False
    else:
        result.append(char)
        start_of_run = True

print(''.join(result))
```
