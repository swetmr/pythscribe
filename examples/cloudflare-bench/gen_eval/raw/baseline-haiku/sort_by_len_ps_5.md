<!-- exp=baseline-haiku task=sort_by_len cond=ps sample=5 model=claude-haiku-4-5-20251001 cost_usd=0.0066222 -->
```python
words = ["fig", "banana", "kiwi", "apple"]
sorted_words = sorted(words, key=lambda w: len(w))
print(sorted_words)
```
