<!-- exp=baseline-sonnet task=str_strip_replace cond=psc sample=1 model=claude-sonnet-5 cost_usd=0.03660000000000001 -->
```python
s = "  hello world  ".strip().replace("world", "pythscribe")
print(f"{s} {len(s)}")
```
