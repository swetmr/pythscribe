<!-- exp=macrov2-sonnet task=macro_kanban_lite cond=psc sample=2 model=claude-sonnet-5 cost_usd=0.0232482 -->
```python
"use client"
R*

FIX = {"todo": ["Write spec", "Design schema"], "doing": ["Build API"], "done": ["Set up repo"]}
COLS = [("todo", "Todo"), ("doing", "Doing"), ("done", "Done")]

@c
def KanbanLite():
    board, set_board = us(FIX)
    drafts, set_drafts = us({"todo": "", "doing": "", "done": ""})

    def move(col, i, dir):
        idx = [c for c, _ in COLS].index(col)
        target = COLS[idx + dir][0]
        card = board[col][i]
        new_board = {**board}
        new_board[col] = [c for j, c in enumerate(board[col]) if j != i]
        new_board[target] = [*board[target], card]
        set_board(new_board)

    def add(col):
        text = drafts[col].strip()
        if text:
            set_board({**board, col: [*board[col], text]})
            set_drafts({**drafts, col: ""})

    def mk_col(col, label):
        idx = [c for c, _ in COLS].index(col)
        cards = board[col]
        return div(cn="kanban-column", key=col,
            h3(label),
            ul(cn="kanban-cards", *[
                li(cn="kanban-card", key=i,
                    span(cn="kanban-title", title),
                    div(cn="kanban-actions",
                        button(oc=lambda: move(col, i, -1), "<") if idx > 0 else None,
                        button(oc=lambda: move(col, i, 1), ">") if idx < len(COLS) - 1 else None,
                    ),
                )
                for i, title in enumerate(cards)
            ]),
            div(cn="kanban-add",
                input(value=drafts[col], ph="New card...",
                    oh=lambda e: set_drafts({**drafts, col: e.target.value})),
                button(oc=lambda: add(col), "Add"),
            ),
        )

    return div(cn="kanban-board", *[mk_col(col, label) for col, label in COLS])
```
