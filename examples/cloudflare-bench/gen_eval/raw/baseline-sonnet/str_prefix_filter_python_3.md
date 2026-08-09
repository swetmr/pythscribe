<!-- exp=baseline-sonnet task=str_prefix_filter cond=python sample=3 model=claude-sonnet-5 cost_usd=0.0086511 -->
```python
words = ["undo", "redo", "unfold", "fold", "unlock"]
result = [w for w in words if w.startswith("un")]
print(result)
```
