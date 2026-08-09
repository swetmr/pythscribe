<!-- exp=baseline-sonnet task=str_strip_replace cond=psc sample=5 model=claude-sonnet-5 cost_usd=0.0097401 -->
```python
s = "  hello world  ".strip().replace("world", "pythscribe")
print(f"{s} {len(s)}")
```
