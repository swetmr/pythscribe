<!-- exp=baseline-sonnet task=str_count_find cond=ps sample=4 model=claude-sonnet-5 cost_usd=0.015401699999999999 -->
```python
s = "abracadabra"
a_count = s.count("a")
bra_count = s.count("bra")
cad_index = s.find("cad")
print(f"{a_count} {bra_count} {cad_index}")
```
