<!-- exp=macrov2-haiku task=macro_kanban_lite cond=psc sample=3 model=claude-haiku-4-5-20251001 cost_usd=0.0559892 -->
```python
"use client"
R*

@c
def KanbanLite():
    columns = ["Todo", "Doing", "Done"]
    initial_cards = {
        "Todo": ["Design mockups", "Create spec"],
        "Doing": ["Implement API"],
        "Done": ["Setup repository"]
    }
    
    cards, set_cards = us(initial_cards)
    inputs, set_inputs = us({"Todo": "", "Doing": "", "Done": ""})
    
    def move_card(col, idx, direction):
        col_idx = columns.index(col)
        if direction == "left" and col_idx > 0:
            new_col = columns[col_idx - 1]
        elif direction == "right" and col_idx < len(columns) - 1:
            new_col = columns[col_idx + 1]
        else:
            return
        
        new_cards = {**cards}
        card = new_cards[col].pop(idx)
        new_cards[new_col].append(card)
        set_cards(new_cards)
    
    def add_card(col, text):
        if not text.strip():
            return
        new_cards = {**cards}
        new_cards[col] = [*new_cards[col], text]
        set_cards(new_cards)
        new_inputs = {**inputs}
        new_inputs[col] = ""
        set_inputs(new_inputs)
    
    def update_input(col, value):
        new_inputs = {**inputs}
        new_inputs[col] = value
        set_inputs(new_inputs)
    
    @c
    def Column(col):
        col_idx = columns.index(col)
        
        def mk_move(idx, direction):
            return lambda: move_card(col, idx, direction)
        
        def mk_add():
            return lambda: add_card(col, inputs[col])
        
        def mk_input():
            return lambda e: update_input(col, e.target.value)
        
        return div(cn="column",
            h2(col),
            div(cn="card-list",
                *[div(cn="card", key=i,
                    div(cn="card-content", card),
                    div(cn="card-buttons",
                        *([button(oc=mk_move(i, "left"), "←")] if col_idx > 0 else []),
                        *([button(oc=mk_move(i, "right"), "→")] if col_idx < len(columns) - 1 else []),
                    )
                ) for i, card in enumerate(cards[col])]
            ),
            div(cn="add-section",
                input(value=inputs[col],
                      oh=mk_input(),
                      ph=f"Add to {col}..."),
                button(oc=mk_add(), "Add"),
            )
        )
    
    return div(cn="kanban",
        *[Column(col) for col in columns]
    )
```
