<!-- exp=baseline-sonnet task=str_upper_join cond=ps sample=5 model=claude-sonnet-5 cost_usd=0.009321900000000001 -->
```python
sentence = "the quick brown fox jumps over the lazy dog"
words = sentence.split()
upper_words = [w.upper() for w in words]
print("|".join(upper_words))
```
