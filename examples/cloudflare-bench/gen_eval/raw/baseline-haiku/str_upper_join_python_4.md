<!-- exp=baseline-haiku task=str_upper_join cond=python sample=4 model=claude-haiku-4-5-20251001 cost_usd=0.0040614 -->
```python
sentence = "the quick brown fox jumps over the lazy dog"
print("|".join(word.upper() for word in sentence.split()))
```
