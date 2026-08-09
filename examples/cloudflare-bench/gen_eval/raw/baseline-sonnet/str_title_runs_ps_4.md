<!-- exp=baseline-sonnet task=str_title_runs cond=ps sample=4 model=claude-sonnet-5 cost_usd=0.0162747 -->
```python
s = "it's a test and don't stop"
result = ""
new_run = True
for c in s:
    if c.isalpha():
        if new_run:
            result += c.upper()
            new_run = False
        else:
            result += c.lower()
    else:
        result += c
        new_run = True
print(result)
```
