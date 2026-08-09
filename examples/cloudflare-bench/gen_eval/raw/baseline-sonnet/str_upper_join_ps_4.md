<!-- exp=baseline-sonnet task=str_upper_join cond=ps sample=4 model=claude-sonnet-5 cost_usd=0.009141900000000001 -->
```python
sentence = "the quick brown fox jumps over the lazy dog"
words = sentence.split()
print("|".join(w.upper() for w in words))
```
