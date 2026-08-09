<!-- exp=baseline-haiku task=str_strip_replace cond=ps sample=2 model=claude-haiku-4-5-20251001 cost_usd=0.0024906 -->
```python
s = "  hello world  ".strip().replace("world", "pythscribe")
print(f"{s} {len(s)}")
```
