<!-- exp=baseline-haiku task=str_strip_replace cond=python sample=1 model=claude-haiku-4-5-20251001 cost_usd=0.0056042999999999996 -->
```python
s = "  hello world  ".strip().replace("world", "pythscribe")
print(f"{s} {len(s)}")
```
