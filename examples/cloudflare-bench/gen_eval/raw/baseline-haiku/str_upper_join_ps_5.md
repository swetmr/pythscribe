<!-- exp=baseline-haiku task=str_upper_join cond=ps sample=5 model=claude-haiku-4-5-20251001 cost_usd=0.0046545 -->
```python
sentence = "the quick brown fox jumps over the lazy dog"
words = sentence.split()
uppercased = [word.upper() for word in words]
print("|".join(uppercased))
```
