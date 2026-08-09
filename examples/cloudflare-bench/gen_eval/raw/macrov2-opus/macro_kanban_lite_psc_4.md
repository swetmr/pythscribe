<!-- exp=macrov2-opus task=macro_kanban_lite cond=psc sample=4 model=claude-opus-4-8 cost_usd=0.03073 -->
```python
"use client"
R*

COLUMNS = ["Todo", "Doing", "Done"]

INITIAL = {
    "Todo": ["Write spec", "Sketch UI"],
    "Doing": ["Build API"],
    "Done": ["Set up repo"],
}

@c
def KanbanLite():
    board, set_board = us(INITIAL)
    drafts, set_drafts = us({"Todo": "", "Doing": "", "Done": ""})

    def move(col_idx, card_idx, delta):
        src = COLUMNS[col_idx]
        dst = COLUMNS[col_idx + delta]
        next_board = {k: [c for c in v] for k, v in board.items()}
        card = next_board[src].pop(card_idx)
        next_board[dst].append(card)
        set_board(next_board)

    def add(col):
        title = drafts[col].strip()
        if title:
            set_board({**board, col: [*board[col], title]})
            set_drafts({**drafts, col: ""})

    return div(cn="kanban",
        h2("KanbanLite"),
        div(cn="columns",
            *[div(key=col, cn="column",
                h3(col),
                ul(cn="cards",
                    *[li(key=f"{col}-{i}", cn="card",
                        span(cn="card-title", title),
                        div(cn="card-actions",
                            (button(cn="move-left",
                                oc=lambda ci=ci, i=i: move(ci, i, -1), "\u2190")
                                if ci > 0 else None),
                            (button(cn="move-right",
                                oc=lambda ci=ci, i=i: move(ci, i, 1), "\u2192")
                                if ci < len(COLUMNS) - 1 else None),
                        ),
                    ) for i, title in enumerate(board[col])]),
                div(cn="add-row",
                    input(cn="add-input", value=drafts[col],
                        ph=f"Add to {col}...",
                        oh=lambda e, col=col: set_drafts({**drafts, col: e.target.value})),
                    button(cn="add-btn", oc=lambda col=col: add(col), "Add"),
                ),
            ) for ci, col in enumerate(COLUMNS)]),
    )
```
