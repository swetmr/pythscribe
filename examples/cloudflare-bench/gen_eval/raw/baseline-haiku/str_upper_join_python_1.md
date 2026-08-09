<!-- exp=baseline-haiku task=str_upper_join cond=python sample=1 model=claude-haiku-4-5-20251001 cost_usd=0.018712 -->
```python
sentence = "the quick brown fox jumps over the lazy dog"
words = sentence.split()
uppercased_words = [word.upper() for word in words]
result = "|".join(uppercased_words)
print(result)
```
