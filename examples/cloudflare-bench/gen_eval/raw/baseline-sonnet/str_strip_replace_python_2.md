<!-- exp=baseline-sonnet task=str_strip_replace cond=python sample=2 model=claude-sonnet-5 cost_usd=0.008037 -->
```python
s = "  hello world  ".strip().replace("world", "pythscribe")
print(s, len(s))
```
