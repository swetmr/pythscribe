<!-- exp=baseline-001 task=str_strip_replace cond=python sample=4 model=claude-opus-4-8 cost_usd=0.012490500000000002 -->
```python
s = "  hello world  ".strip().replace("world", "pythscribe")
print(f"{s} {len(s)}")
```
