<!-- exp=baseline-sonnet task=str_strip_replace cond=psc sample=4 model=claude-sonnet-5 cost_usd=0.0093501 -->
```python
s = "  hello world  ".strip().replace("world", "pythscribe")
print(s, len(s))
```
