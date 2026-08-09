<!-- exp=baseline-haiku task=str_upper_join cond=ps sample=1 model=claude-haiku-4-5-20251001 cost_usd=0.0103693 -->
```python
sentence = "the quick brown fox jumps over the lazy dog"
print("|".join(w.upper() for w in sentence.split()))
```
