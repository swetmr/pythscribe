<!-- exp=baseline-sonnet task=str_prefix_filter cond=psc sample=4 model=claude-sonnet-5 cost_usd=0.0095142 -->
```python
words = ["undo", "redo", "unfold", "fold", "unlock"]
print([w for w in words if w.startswith("un")])
```
