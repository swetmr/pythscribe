<!-- exp=baseline-001 task=str_upper_join cond=ps sample=3 model=claude-opus-4-8 cost_usd=0.010607 -->
```python
sentence = "the quick brown fox jumps over the lazy dog"
words = [word.upper() for word in sentence.split()]
print("|".join(words))
```
