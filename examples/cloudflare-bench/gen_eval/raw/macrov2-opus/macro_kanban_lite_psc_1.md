<!-- exp=macrov2-opus task=macro_kanban_lite cond=psc sample=1 model=claude-opus-4-8 cost_usd=0.107574 -->
```python
"use client"
R*

COLUMNS = ["Todo", "Doing", "Done"]

@c
def KanbanLite():
    board, set_board = us([
        ["Buy milk", "Walk the dog"],
        ["Write report"],
        ["Ship v1"],
    ])
    drafts, set_drafts = us(["", "", ""])

    def move(col, idx, delta):
        next_board = [list(c) for c in board]
        card = next_board[col].pop(idx)
        next_board[col + delta].append(card)
        set_board(next_board)

    def add(col):
        title = drafts[col].strip()
        if not title:
            return
        next_board = [list(c) for c in board]
        next_board[col].append(title)
        set_board(next_board)
        next_drafts = list(drafts)
        next_drafts[col] = ""
        set_drafts(next_drafts)

    def set_draft(col, value):
        next_drafts = list(drafts)
        next_drafts[col] = value
        set_drafts(next_drafts)

    return div(cn="kanban",
        *[div(key=col, cn="column",
            h2(cn="column-title", COLUMNS[col]),
            ul(cn="cards",
                *[li(key=idx, cn="card",
                    span(cn="card-title", title),
                    div(cn="card-actions",
                        button(cn="move-left", oc=lambda: move(col, idx, -1), "←") if col > 0 else None,
                        button(cn="move-right", oc=lambda: move(col, idx, 1), "→") if col < len(COLUMNS) - 1 else None,
                    ),
                ) for idx, title in enumerate(cards)]
            ),
            div(cn="add-card",
                input(cn="add-input", value=drafts[col],
                      oh=lambda e: set_draft(col, e.target.value),
                      ph="New card..."),
                button(cn="add-btn", oc=lambda: add(col), "Add"),
            ),
        ) for col, cards in enumerate(board)]
    )
```
