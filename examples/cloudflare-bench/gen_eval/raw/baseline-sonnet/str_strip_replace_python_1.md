<!-- exp=baseline-sonnet task=str_strip_replace cond=python sample=1 model=claude-sonnet-5 cost_usd=0.009918 -->
```python
s = "  hello world  ".strip().replace("world", "pythscribe")
print(s, len(s))
```
