<!-- exp=baseline-haiku task=str_prefix_filter cond=psc sample=4 model=claude-haiku-4-5-20251001 cost_usd=0.006055100000000001 -->
```python
words = ["undo", "redo", "unfold", "fold", "unlock"]
result = [w for w in words if w.startswith("un")]
print(result)
```
