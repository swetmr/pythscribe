<!-- exp=baseline-haiku task=str_strip_replace cond=psc sample=2 model=claude-haiku-4-5-20251001 cost_usd=0.0035398 -->
```python
s = "  hello world  "
s = s.strip()
s = s.replace("world", "pythscribe")
print(s, len(s))
```
