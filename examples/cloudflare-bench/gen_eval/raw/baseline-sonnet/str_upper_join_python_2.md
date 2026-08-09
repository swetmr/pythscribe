<!-- exp=baseline-sonnet task=str_upper_join cond=python sample=2 model=claude-sonnet-5 cost_usd=0.0423567 -->
```python
sentence = "the quick brown fox jumps over the lazy dog"
print("|".join(word.upper() for word in sentence.split()))
```
