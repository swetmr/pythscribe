<!-- exp=baseline-sonnet task=even_square_sum cond=ps sample=4 model=claude-sonnet-5 cost_usd=0.0110277 -->
```python
total = sum(n ** 2 for n in range(1, 21) if n % 2 == 0)
print(total)
```
