<!-- exp=baseline-001 task=str_prefix_filter cond=python sample=3 model=claude-opus-4-8 cost_usd=0.010414 -->
```python
words = ["undo", "redo", "unfold", "fold", "unlock"]
print([w for w in words if w.startswith("un")])
```
