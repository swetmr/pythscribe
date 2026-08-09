<!-- exp=baseline-sonnet task=str_strip_replace cond=python sample=5 model=claude-sonnet-5 cost_usd=0.008097 -->
```python
s = "  hello world  "
s = s.strip().replace("world", "pythscribe")
print(s, len(s))
```
