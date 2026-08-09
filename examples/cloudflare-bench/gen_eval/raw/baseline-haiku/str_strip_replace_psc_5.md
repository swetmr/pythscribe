<!-- exp=baseline-haiku task=str_strip_replace cond=psc sample=5 model=claude-haiku-4-5-20251001 cost_usd=0.0067198000000000015 -->
```python
s = "  hello world  "
result = s.strip().replace("world", "pythscribe")
print(result, len(result))
```
