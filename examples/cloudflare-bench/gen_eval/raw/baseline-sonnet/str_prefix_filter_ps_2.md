<!-- exp=baseline-sonnet task=str_prefix_filter cond=ps sample=2 model=claude-sonnet-5 cost_usd=0.0091602 -->
```python
words = ["undo", "redo", "unfold", "fold", "unlock"]
result = [w for w in words if w.startswith("un")]
print(result)
```
