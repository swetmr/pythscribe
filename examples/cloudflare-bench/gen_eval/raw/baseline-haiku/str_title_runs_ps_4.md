<!-- exp=baseline-haiku task=str_title_runs cond=ps sample=4 model=claude-haiku-4-5-20251001 cost_usd=0.0151315 -->
```python
text = "it's a test and don't stop"
result = []
i = 0

while i < len(text):
    if text[i].isalpha():
        result.append(text[i].upper())
        i += 1
        while i < len(text) and text[i].isalpha():
            result.append(text[i].lower())
            i += 1
    else:
        result.append(text[i])
        i += 1

print(''.join(result))
```
