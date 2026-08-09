<!-- exp=baseline-001 task=str_strip_replace cond=ps sample=1 model=claude-opus-4-8 cost_usd=0.0409705 -->
```python
s = "  hello world  ".strip().replace("world", "pythscribe")
print(f"{s} {len(s)}")
```
