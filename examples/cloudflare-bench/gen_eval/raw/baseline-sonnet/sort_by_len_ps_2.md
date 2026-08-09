<!-- exp=baseline-sonnet task=sort_by_len cond=ps sample=2 model=claude-sonnet-5 cost_usd=0.0092292 -->
```python
words = ["fig", "banana", "kiwi", "apple"]
sorted_words = sorted(words, key=lambda w: len(w))
print(sorted_words)
```
