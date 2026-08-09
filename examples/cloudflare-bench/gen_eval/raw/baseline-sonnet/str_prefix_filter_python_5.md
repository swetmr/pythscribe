<!-- exp=baseline-sonnet task=str_prefix_filter cond=python sample=5 model=claude-sonnet-5 cost_usd=0.008576100000000001 -->
```python
words = ["undo", "redo", "unfold", "fold", "unlock"]
print([w for w in words if w.startswith("un")])
```
