<!-- exp=baseline-haiku task=str_upper_join cond=ps sample=2 model=claude-haiku-4-5-20251001 cost_usd=0.0030045000000000002 -->
```python
sentence = "the quick brown fox jumps over the lazy dog"
print("|".join(word.upper() for word in sentence.split()))
```
