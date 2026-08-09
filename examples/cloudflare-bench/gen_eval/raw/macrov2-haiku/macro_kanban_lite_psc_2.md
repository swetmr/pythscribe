<!-- exp=macrov2-haiku task=macro_kanban_lite cond=psc sample=2 model=claude-haiku-4-5-20251001 cost_usd=0.0498892 -->
```python
"use client"
R*

@c
def KanbanLite():
    cards, set_cards = us({
        "todo": ["Design mockups", "Write specs"],
        "doing": ["Build API"],
        "done": ["Setup project"]
    })
    inputs, set_inputs = us({"todo": "", "doing": "", "done": ""})
    cols = ["todo", "doing", "done"]
    
    def add_card(col):
        text = inputs[col].strip()
        if text:
            new_cards = {**cards}
            new_cards[col] = [*new_cards[col], text]
            set_cards(new_cards)
            set_inputs({**inputs, col: ""})
    
    def move_card(from_col, to_col, idx):
        new_cards = {**cards}
        card = new_cards[from_col].pop(idx)
        new_cards[to_col].append(card)
        set_cards(new_cards)
    
    return div(cn="kanban",
        *[Column(col, cols, cards[col], inputs[col], add_card, move_card, set_inputs, inputs)
          for col in cols]
    )

@c
def Column(col, cols, card_list, input_val, on_add, on_move, set_inputs, inputs):
    col_idx = cols.index(col)
    return div(cn="column",
        h2(col.capitalize()),
        input(ph=f"Add {col}...", value=input_val, oh=lambda e: set_inputs({**inputs, col: e.target.value})),
        button(oc=lambda: on_add(col), "Add"),
        div(cn="cards",
            *[div(cn="card",
                col_idx > 0 and button(oc=lambda c=col, nc=cols[col_idx-1], i=idx: on_move(c, nc, i), "←"),
                span(cn="card-title", card),
                col_idx < len(cols) - 1 and button(oc=lambda c=col, nc=cols[col_idx+1], i=idx: on_move(c, nc, i), "→")
            ) for idx, card in enumerate(card_list)]
        )
    )
```
