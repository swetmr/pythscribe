<!-- exp=macrov2-sonnet task=macro_kanban_lite cond=psc sample=4 model=claude-sonnet-5 cost_usd=0.0630732 -->
```python
"use client"
R*

COLUMNS = ["Todo", "Doing", "Done"]

INITIAL_CARDS = {
    "Todo": [{"id": 1, "title": "Write spec"}, {"id": 2, "title": "Set up repo"}],
    "Doing": [{"id": 3, "title": "Build API"}],
    "Done": [{"id": 4, "title": "Kickoff meeting"}],
}

@c
def KanbanLite():
    cards, set_cards = us(INITIAL_CARDS)
    inputs, set_inputs = us({"Todo": "", "Doing": "", "Done": ""})
    next_id, set_next_id = us(5)

    def move(col, card_id, direction):
        idx = COLUMNS.index(col)
        target_idx = idx + direction
        if target_idx < 0 or target_idx >= len(COLUMNS):
            return
        target_col = COLUMNS[target_idx]
        card = None
        for c in cards[col]:
            if c["id"] == card_id:
                card = c
        updated = {**cards}
        updated[col] = [c for c in cards[col] if c["id"] != card_id]
        updated[target_col] = [*cards[target_col], card]
        set_cards(updated)

    def add_card(col):
        title = inputs[col].strip()
        if not title:
            return
        set_cards({**cards, col: [*cards[col], {"id": next_id, "title": title}]})
        set_inputs({**inputs, col: ""})
        set_next_id(next_id + 1)

    def make_input_handler(col):
        return lambda e: set_inputs({**inputs, col: e.target.value})

    def make_key_handler(col):
        return lambda e: add_card(col) if e.key == "Enter" else None

    def make_add_handler(col):
        return lambda: add_card(col)

    def make_move_handler(col, card_id, direction):
        return lambda: move(col, card_id, direction)

    return div(cn="kanban-board",
        *[
            div(key=col, cn="kanban-column",
                h3(cn="kanban-column-title", col),
                ul(cn="kanban-cards",
                    *[
                        li(key=card["id"], cn="kanban-card",
                            span(cn="kanban-card-title", card["title"]),
                            div(cn="kanban-card-actions",
                                button(cn="kanban-move-btn", oc=make_move_handler(col, card["id"], -1), "<")
                                if COLUMNS.index(col) > 0 else None,
                                button(cn="kanban-move-btn", oc=make_move_handler(col, card["id"], 1), ">")
                                if COLUMNS.index(col) < len(COLUMNS) - 1 else None,
                            ),
                        )
                        for card in cards[col]
                    ],
                ),
                div(cn="kanban-add",
                    input(value=inputs[col], oh=make_input_handler(col),
                          ph="New card...", on_key_down=make_key_handler(col)),
                    button(cn="kanban-add-btn", oc=make_add_handler(col), "Add"),
                ),
            )
            for col in COLUMNS
        ],
    )
```
