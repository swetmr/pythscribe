<!-- exp=baseline-sonnet task=str_prefix_filter cond=psc sample=3 model=claude-sonnet-5 cost_usd=0.0124842 -->
```python
words = ["undo", "redo", "unfold", "fold", "unlock"]
print([w for w in words if w.startswith("un")])
```
