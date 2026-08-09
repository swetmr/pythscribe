<!-- exp=baseline-haiku task=str_title_runs cond=python sample=2 model=claude-haiku-4-5-20251001 cost_usd=0.0170384 -->
```python
text = "it's a test and don't stop"
result = []
current_run = []

for char in text:
    if char.isalpha():
        current_run.append(char)
    else:
        if current_run:
            run_str = ''.join(current_run)
            result.append(run_str[0].upper() + run_str[1:].lower())
            current_run = []
        result.append(char)

if current_run:
    run_str = ''.join(current_run)
    result.append(run_str[0].upper() + run_str[1:].lower())

print(''.join(result))
```
