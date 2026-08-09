<!-- exp=baseline-sonnet task=str_upper_join cond=psc sample=5 model=claude-sonnet-5 cost_usd=0.0097959 -->
```python
s = "the quick brown fox jumps over the lazy dog"
print("|".join(w.upper() for w in s.split()))
```
