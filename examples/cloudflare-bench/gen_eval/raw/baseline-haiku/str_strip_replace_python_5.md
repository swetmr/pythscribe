<!-- exp=baseline-haiku task=str_strip_replace cond=python sample=5 model=claude-haiku-4-5-20251001 cost_usd=0.0046575 -->
```python
s = "  hello world  "
s = s.strip()
s = s.replace("world", "pythscribe")
print(s, len(s))
```
