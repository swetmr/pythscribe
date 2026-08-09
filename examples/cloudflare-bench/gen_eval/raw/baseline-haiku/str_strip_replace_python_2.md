<!-- exp=baseline-haiku task=str_strip_replace cond=python sample=2 model=claude-haiku-4-5-20251001 cost_usd=0.0034975 -->
```python
s = "  hello world  "
s = s.strip()
s = s.replace("world", "pythscribe")
print(f"{s} {len(s)}")
```
