<!-- exp=baseline-sonnet task=str_strip_replace cond=ps sample=5 model=claude-sonnet-5 cost_usd=0.009056100000000001 -->
```python
s = "  hello world  "
s = s.strip().replace("world", "pythscribe")
print(f"{s} {len(s)}")
```
