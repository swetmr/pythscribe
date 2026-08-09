<!-- exp=baseline-sonnet task=str_count_find cond=ps sample=5 model=claude-sonnet-5 cost_usd=0.0103617 -->
```python
s = "abracadabra"
count_a = s.count("a")
count_bra = s.count("bra")
index_cad = s.find("cad")
print(f"{count_a} {count_bra} {index_cad}")
```
