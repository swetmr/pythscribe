<!-- exp=baseline-001 task=str_strip_replace cond=ps sample=2 model=claude-opus-4-8 cost_usd=0.010414 -->
```python
s = "  hello world  "
s = s.strip().replace("world", "pythscribe")
print(f"{s} {len(s)}")
```
