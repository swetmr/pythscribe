<!-- exp=baseline-haiku task=iter_combinations cond=python sample=1 model=claude-haiku-4-5-20251001 cost_usd=0.0075353 -->
```python
s = "abcd"
for i in range(len(s)):
    for j in range(i + 1, len(s)):
        print((s[i], s[j]))
```
