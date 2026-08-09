<!-- exp=baseline-sonnet task=str_prefix_filter cond=ps sample=1 model=claude-sonnet-5 cost_usd=0.027537 -->
```python
words = ["undo", "redo", "unfold", "fold", "unlock"]
result = [w for w in words if w.startswith("un")]
print(result)
```
