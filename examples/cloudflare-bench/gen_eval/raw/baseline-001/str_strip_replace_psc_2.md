<!-- exp=baseline-001 task=str_strip_replace cond=psc sample=2 model=claude-opus-4-8 cost_usd=0.011054 -->
```python
s = "  hello world  ".strip().replace("world", "pythscribe")
print(f"{s} {len(s)}")
```
