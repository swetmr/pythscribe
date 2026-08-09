<!-- exp=baseline-haiku task=str_prefix_filter cond=psc sample=3 model=claude-haiku-4-5-20251001 cost_usd=0.0037301 -->
```python
words = ["undo", "redo", "unfold", "fold", "unlock"]
result = [w for w in words if w.startswith("un")]
print(result)
```
