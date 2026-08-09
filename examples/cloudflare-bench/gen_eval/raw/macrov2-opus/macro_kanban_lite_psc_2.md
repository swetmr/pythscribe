<!-- exp=macrov2-opus task=macro_kanban_lite cond=psc sample=2 model=claude-opus-4-8 cost_usd=0.03798 -->
```python
"use client"
R*

COLUMNS = ["Todo", "Doing", "Done"]

@c
def KanbanLite():
    board, set_board = us([
        ["Write spec", "Sketch UI"],
        ["Build API"],
        ["Setup repo"],
    ])
    drafts, set_drafts = us(["", "", ""])

    def move(col, idx, direction):
        target = col + direction
        next_board = [[t for t in c] for c in board]
        title = next_board[col].pop(idx)
        next_board[target].append(title)
        set_board(next_board)

    def set_draft(col, value):
        next_drafts = [d for d in drafts]
        next_drafts[col] = value
        set_drafts(next_drafts)

    def add(col):
        title = drafts[col].strip()
        if title:
            next_board = [[t for t in c] for c in board]
            next_board[col].append(title)
            set_board(next_board)
            set_draft(col, "")

    return div(cn="kanban",
        h2("KanbanLite"),
        div(cn="kanban-columns",
            *[div(key=name, cn="kanban-column",
                h3(name),
                ul(cn="kanban-cards",
                    *[li(key=f"{col}-{idx}", cn="kanban-card",
                        span(cn="kanban-title", title),
                        div(cn="kanban-actions",
                            (button(cn="move-left", oc=lambda: move(col, idx, -1), "\u2190")
                                if col > 0 else None),
                            (button(cn="move-right", oc=lambda: move(col, idx, 1), "\u2192")
                                if col < len(COLUMNS) - 1 else None),
                        ),
                    ) for idx, title in enumerate(board[col])]),
                div(cn="kanban-add",
                    input(cn="kanban-input", value=drafts[col],
                        oh=lambda e, col=col: set_draft(col, e.target.value),
                        ph=f"Add to {name}..."),
                    button(cn="kanban-add-btn", oc=lambda: add(col), "Add"),
                ),
            ) for col, name in enumerate(COLUMNS)]),
    )
```
