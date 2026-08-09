<!-- exp=baseline-haiku task=macro_kanban_lite cond=psc sample=4 model=claude-haiku-4-5-20251001 cost_usd=0.0425667 -->
```python
"use client"
R*

@c
def KanbanLite():
    cards, set_cards = us({
        "Todo": ["Design mockups", "Write spec"],
        "Doing": ["Implement auth"],
        "Done": ["Setup project"]
    })
    
    inputs = {
        "Todo": us(""),
        "Doing": us(""),
        "Done": us("")
    }
    
    def move_card(from_col, to_col, idx):
        new = {k: list(v) for k, v in cards.items()}
        card = new[from_col].pop(idx)
        new[to_col].append(card)
        set_cards(new)
    
    def add_card(col, text, setter):
        if text.strip():
            new = {k: list(v) for k, v in cards.items()}
            new[col].append(text)
            set_cards(new)
            setter("")
    
    def column(name, prev, nxt):
        col_cards = cards[name]
        val, set_val = inputs[name]
        return div(cn="column",
            h2(name),
            input(ph="New card...", value=val, oh=lambda e: set_val(e.target.value)),
            button(oc=lambda: add_card(name, val, set_val), "Add"),
            div(cn="cards",
                *[div(cn="card", key=i,
                    (button(oc=lambda i=i: move_card(name, prev, i), "←") if prev else None),
                    span(col_cards[i]),
                    (button(oc=lambda i=i: move_card(name, nxt, i), "→") if nxt else None),
                  ) for i in range(len(col_cards))]
            )
        )
    
    return div(cn="kanban",
        column("Todo", None, "Doing"),
        column("Doing", "Todo", "Done"),
        column("Done", "Doing", None),
    )
```
