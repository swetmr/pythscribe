<!-- exp=baseline-haiku task=str_prefix_filter cond=ps sample=2 model=claude-haiku-4-5-20251001 cost_usd=0.0045909 -->
```python
words = ["undo", "redo", "unfold", "fold", "unlock"]
result = [w for w in words if w.startswith("un")]
print(result)
```
