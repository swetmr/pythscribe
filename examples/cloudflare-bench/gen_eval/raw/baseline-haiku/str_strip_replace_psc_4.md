<!-- exp=baseline-haiku task=str_strip_replace cond=psc sample=4 model=claude-haiku-4-5-20251001 cost_usd=0.0053098 -->
```python
s = "  hello world  ".strip().replace("world", "pythscribe")
print(f"{s} {len(s)}")
```
