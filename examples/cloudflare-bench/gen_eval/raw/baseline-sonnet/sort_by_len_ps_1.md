<!-- exp=baseline-sonnet task=sort_by_len cond=ps sample=1 model=claude-sonnet-5 cost_usd=0.027492 -->
```python
words = ["fig", "banana", "kiwi", "apple"]
sorted_words = sorted(words, key=lambda w: len(w))
print(sorted_words)
```
