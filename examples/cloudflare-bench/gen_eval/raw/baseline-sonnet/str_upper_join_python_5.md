<!-- exp=baseline-sonnet task=str_upper_join cond=python sample=5 model=claude-sonnet-5 cost_usd=0.008212800000000001 -->
```python
sentence = "the quick brown fox jumps over the lazy dog"
print("|".join(word.upper() for word in sentence.split()))
```
