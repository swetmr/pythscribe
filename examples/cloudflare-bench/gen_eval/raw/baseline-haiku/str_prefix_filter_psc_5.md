<!-- exp=baseline-haiku task=str_prefix_filter cond=psc sample=5 model=claude-haiku-4-5-20251001 cost_usd=0.0074451 -->
```python
words = ["undo", "redo", "unfold", "fold", "unlock"]
print([w for w in words if w.startswith("un")])
```
