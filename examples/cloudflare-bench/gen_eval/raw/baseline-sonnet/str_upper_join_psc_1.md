<!-- exp=baseline-sonnet task=str_upper_join cond=psc sample=1 model=claude-sonnet-5 cost_usd=0.036216000000000005 -->
```python
s = "the quick brown fox jumps over the lazy dog"
print("|".join(w.upper() for w in s.split()))
```
