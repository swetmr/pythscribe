<!-- exp=baseline-001 task=str_prefix_filter cond=python sample=1 model=claude-opus-4-8 cost_usd=0.013370500000000002 -->
```python
words = ["undo", "redo", "unfold", "fold", "unlock"]
print([w for w in words if w.startswith("un")])
```
