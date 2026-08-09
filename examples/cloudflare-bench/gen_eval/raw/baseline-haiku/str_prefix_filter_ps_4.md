<!-- exp=baseline-haiku task=str_prefix_filter cond=ps sample=4 model=claude-haiku-4-5-20251001 cost_usd=0.0068709 -->
I'll write a simple PythScribe program to filter the list for words starting with "un":

```python
words = ["undo", "redo", "unfold", "fold", "unlock"]
result = [w for w in words if w.startswith("un")]
print(result)
```
