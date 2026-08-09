<!-- exp=baseline-haiku task=counter_most_common cond=python sample=2 model=claude-haiku-4-5-20251001 cost_usd=0.007647600000000001 -->
```python
text = "mississippi"

letter_counts = {}
first_appearance = {}
for i, letter in enumerate(text):
    if letter not in letter_counts:
        letter_counts[letter] = 0
        first_appearance[letter] = i
    letter_counts[letter] += 1

sorted_letters = sorted(letter_counts.items(), key=lambda x: (-x[1], first_appearance[x[0]]))
print(sorted_letters[:3])
```
