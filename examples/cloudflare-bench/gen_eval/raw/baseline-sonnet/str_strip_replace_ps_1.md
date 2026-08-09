<!-- exp=baseline-sonnet task=str_strip_replace cond=ps sample=1 model=claude-sonnet-5 cost_usd=0.028634999999999997 -->
```python
s = "  hello world  "
s = s.strip()
s = s.replace("world", "pythscribe")
print(f"{s} {len(s)}")
```
