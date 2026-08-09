<!-- exp=baseline-001 task=macro_kanban_lite cond=psc sample=1 model=claude-opus-4-8 cost_usd=0.1105305 -->
```python
"use client"
R*

COLUMNS = ["Todo", "Doing", "Done"]

@c
def KanbanLite():
    board, set_board = us({
        "Todo": ["Write spec", "Sketch UI"],
        "Doing": ["Build API"],
        "Done": ["Setup repo"],
    })
    drafts, set_drafts = us({"Todo": "", "Doing": "", "Done": ""})

    def move(col_i, card_i, delta):
        src = COLUMNS[col_i]
        dst = COLUMNS[col_i + delta]
        next_board = {k: [c for c in v] for k, v in board.items()}
        card = next_board[src].pop(card_i)
        next_board[dst].append(card)
        set_board(next_board)

    def add(col):
        title = drafts[col].strip()
        if title:
            set_board({**board, col: [*board[col], title]})
            set_drafts({**drafts, col: ""})

    return div(cn="kanban",
        *[div(key=col, cn="kanban-column",
            h2(cn="kanban-title", col),
            ul(cn="kanban-cards",
                *[li(key=f"{col}-{ci}", cn="kanban-card",
                    span(cn="kanban-card-title", title),
                    div(cn="kanban-card-actions",
                        button(oc=lambda: move(coli, ci, -1), cn="move-left", "←") if coli > 0 else None,
                        button(oc=lambda: move(coli, ci, 1), cn="move-right", "→") if coli < len(COLUMNS) - 1 else None,
                    ),
                ) for ci, title in enumerate(board[col])]),
            div(cn="kanban-add",
                input(value=drafts[col],
                      oh=lambda e: set_drafts({**drafts, col: e.target.value}),
                      ph="New card..."),
                button(oc=lambda: add(col), cn="kanban-add-btn", "Add"),
            ),
        ) for coli, col in enumerate(COLUMNS)],
    )
```
