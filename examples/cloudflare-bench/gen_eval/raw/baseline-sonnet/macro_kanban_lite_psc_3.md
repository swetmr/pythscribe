<!-- exp=baseline-sonnet task=macro_kanban_lite cond=psc sample=3 model=claude-sonnet-5 cost_usd=0.058695899999999995 -->
```python
"use client"
R*

COLUMNS = ["Todo", "Doing", "Done"]

INITIAL_CARDS = {
    "Todo": ["Design schema", "Write tests"],
    "Doing": ["Build API"],
    "Done": ["Set up repo"],
}

def move_card(cards, col_idx, card_idx, direction):
    new_cards = {c: list(cards[c]) for c in cards}
    src = COLUMNS[col_idx]
    dst = COLUMNS[col_idx + direction]
    title = new_cards[src].pop(card_idx)
    new_cards[dst].append(title)
    return new_cards

def add_card(cards, col, title):
    new_cards = {c: list(cards[c]) for c in cards}
    if title.strip():
        new_cards[col].append(title.strip())
    return new_cards

@c
def KanbanLite():
    cards, set_cards = us(INITIAL_CARDS)
    inputs, set_inputs = us({"Todo": "", "Doing": "", "Done": ""})

    def move(col_idx, card_idx, direction):
        set_cards(move_card(cards, col_idx, card_idx, direction))

    def add(col):
        set_cards(add_card(cards, col, inputs[col]))
        set_inputs({**inputs, col: ""})

    def set_input(col, value):
        set_inputs({**inputs, col: value})

    return div(cn="kanban-board",
        *[
            div(key=col, cn="kanban-column",
                h3(cn="kanban-column-title", col),
                ul(cn="kanban-list",
                    *[
                        li(key=i, cn="kanban-card",
                            span(cn="kanban-card-title", title),
                            div(cn="kanban-card-actions",
                                button(cn="kanban-move-left",
                                       oc=lambda col_idx=col_idx, i=i: move(col_idx, i, -1),
                                       "<") if col_idx > 0 else None,
                                button(cn="kanban-move-right",
                                       oc=lambda col_idx=col_idx, i=i: move(col_idx, i, 1),
                                       ">") if col_idx < len(COLUMNS) - 1 else None,
                            ),
                        )
                        for i, title in enumerate(cards[col])
                    ],
                ),
                div(cn="kanban-add",
                    input(cn="kanban-add-input", value=inputs[col],
                          oh=lambda e, col=col: set_input(col, e.target.value),
                          ph="New card..."),
                    button(cn="kanban-add-button", oc=lambda col=col: add(col), "Add"),
                ),
            )
            for col_idx, col in enumerate(COLUMNS)
        ],
    )
```
