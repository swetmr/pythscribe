<!-- exp=baseline-haiku task=macro_kanban_lite cond=psc sample=5 model=claude-haiku-4-5-20251001 cost_usd=0.045396700000000005 -->
```python
"use client"
R*

@c
def KanbanLite():
    cards, set_cards = us({
        "todo": ["Learn PythScribe", "Build components"],
        "doing": ["Code Kanban"],
        "done": ["Set up project"]
    })
    
    input_texts, set_input_texts = us({"todo": "", "doing": "", "done": ""})
    
    def move_card(col, idx, direction):
        cols = ["todo", "doing", "done"]
        col_idx = cols.index(col)
        
        if direction == "left" and col_idx > 0:
            new_cards = {k: v[:] for k, v in cards.items()}
            card = new_cards[col].pop(idx)
            new_cards[cols[col_idx - 1]].append(card)
            set_cards(new_cards)
        elif direction == "right" and col_idx < 2:
            new_cards = {k: v[:] for k, v in cards.items()}
            card = new_cards[col].pop(idx)
            new_cards[cols[col_idx + 1]].append(card)
            set_cards(new_cards)
    
    def add_card(col, text):
        if text.strip():
            new_cards = {k: v[:] for k, v in cards.items()}
            new_cards[col].append(text)
            set_cards(new_cards)
            new_inputs = {k: v for k, v in input_texts.items()}
            new_inputs[col] = ""
            set_input_texts(new_inputs)
    
    def render_column(col):
        cols = ["todo", "doing", "done"]
        col_idx = cols.index(col)
        col_titles = {"todo": "Todo", "doing": "Doing", "done": "Done"}
        
        return div(cn="column",
            h3(cn="column-title", col_titles[col]),
            div(cn="column-input-group",
                input(
                    value=input_texts.get(col, ""),
                    oh=lambda e, c=col: set_input_texts({**input_texts, c: e.target.value}),
                    ph="Add new card...",
                    cn="column-input"
                ),
                button(
                    oc=lambda c=col: add_card(c, input_texts.get(c, "")),
                    cn="add-btn",
                    "Add"
                ),
            ),
            div(cn="cards-list",
                *[div(cn="card",
                    div(cn="card-text", card),
                    div(cn="card-actions",
                        button(
                            oc=lambda c=col, i=idx: move_card(c, i, "left"),
                            cn="move-btn",
                            "←"
                        ) if col_idx > 0 else None,
                        button(
                            oc=lambda c=col, i=idx: move_card(c, i, "right"),
                            cn="move-btn",
                            "→"
                        ) if col_idx < 2 else None,
                    )
                ) for idx, card in enumerate(cards.get(col, []))]
            )
        )
    
    return div(cn="kanban-lite",
        div(cn="kanban-board",
            render_column("todo"),
            render_column("doing"),
            render_column("done"),
        )
    )
```
