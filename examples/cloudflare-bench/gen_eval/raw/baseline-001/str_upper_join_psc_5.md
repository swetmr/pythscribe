<!-- exp=baseline-001 task=str_upper_join cond=psc sample=5 model=claude-opus-4-8 cost_usd=0.011097000000000001 -->
```python
s = "the quick brown fox jumps over the lazy dog"
print("|".join(w.upper() for w in s.split()))
```
