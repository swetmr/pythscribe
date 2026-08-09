<!-- exp=baseline-haiku task=str_prefix_filter cond=python sample=5 model=claude-haiku-4-5-20251001 cost_usd=0.0055878 -->
```python
words = ["undo", "redo", "unfold", "fold", "unlock"]
result = [word for word in words if word.startswith("un")]
print(result)
```
