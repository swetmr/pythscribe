<!-- exp=baseline-haiku task=str_count_find cond=ps sample=3 model=claude-haiku-4-5-20251001 cost_usd=0.0113665 -->
```python
s = "abracadabra"
a_count = s.count("a")
bra_count = s.count("bra")
cad_index = s.find("cad")
print(f"{a_count} {bra_count} {cad_index}")
```
