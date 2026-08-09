<!-- exp=baseline-haiku task=str_title_runs cond=ps sample=1 model=claude-haiku-4-5-20251001 cost_usd=0.0290143 -->
```python
s = "it's a test and don't stop"
result = []

for i, char in enumerate(s):
    if char.isalpha():
        if i == 0 or not s[i-1].isalpha():
            result.append(char.upper())
        else:
            result.append(char.lower())
    else:
        result.append(char)

print("".join(result))
```
