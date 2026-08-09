<!-- exp=baseline-sonnet task=str_count_find cond=python sample=5 model=claude-sonnet-5 cost_usd=0.0088176 -->
```python
s = "abracadabra"
count_a = s.count("a")
count_bra = s.count("bra")
idx_cad = s.find("cad")
print(count_a, count_bra, idx_cad)
```
