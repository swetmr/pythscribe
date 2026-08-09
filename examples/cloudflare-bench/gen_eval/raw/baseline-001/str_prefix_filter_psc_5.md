<!-- exp=baseline-001 task=str_prefix_filter cond=psc sample=5 model=claude-opus-4-8 cost_usd=0.0111775 -->
```python
words = ["undo", "redo", "unfold", "fold", "unlock"]
print([w for w in words if w.startswith("un")])
```
