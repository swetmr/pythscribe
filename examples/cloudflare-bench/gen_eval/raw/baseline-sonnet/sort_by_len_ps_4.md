<!-- exp=baseline-sonnet task=sort_by_len cond=ps sample=4 model=claude-sonnet-5 cost_usd=0.0092292 -->
```python
words = ["fig", "banana", "kiwi", "apple"]
words_sorted = sorted(words, key=lambda w: len(w))
print(words_sorted)
```
