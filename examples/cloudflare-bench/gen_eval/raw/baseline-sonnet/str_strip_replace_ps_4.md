<!-- exp=baseline-sonnet task=str_strip_replace cond=ps sample=4 model=claude-sonnet-5 cost_usd=0.0089961 -->
```python
s = "  hello world  ".strip().replace("world", "pythscribe")
print(f"{s} {len(s)}")
```
