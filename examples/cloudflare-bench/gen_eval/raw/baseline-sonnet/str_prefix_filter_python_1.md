<!-- exp=baseline-sonnet task=str_prefix_filter cond=python sample=1 model=claude-sonnet-5 cost_usd=0.010065000000000001 -->
```python
words = ["undo", "redo", "unfold", "fold", "unlock"]
print([w for w in words if w.startswith("un")])
```
