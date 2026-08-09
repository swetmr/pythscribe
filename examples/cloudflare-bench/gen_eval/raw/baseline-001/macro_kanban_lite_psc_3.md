<!-- exp=baseline-001 task=macro_kanban_lite cond=psc sample=3 model=claude-opus-4-8 cost_usd=0.034847 -->
```python
"use client"
R*

COLUMNS = ["Todo", "Doing", "Done"]

def initial_board():
    return {
        "Todo": ["Write spec", "Sketch UI"],
        "Doing": ["Build API"],
        "Done": ["Set up repo"],
    }

@c
def KanbanLite():
    board, set_board = us(initial_board())
    drafts, set_drafts = us({c: "" for c in COLUMNS})

    def move(col, idx, delta):
        target = COLUMNS[COLUMNS.index(col) + delta]
        next_board = {c: [t for t in board[c]] for c in COLUMNS}
        card = next_board[col].pop(idx)
        next_board[target].append(card)
        set_board(next_board)

    def set_draft(col, value):
        set_drafts({**drafts, col: value})

    def add(col):
        title = drafts[col].strip()
        if title:
            set_board({**board, col: [*board[col], title]})
            set_draft(col, "")

    def render_card(col, idx, title):
        col_i = COLUMNS.index(col)
        return li(key=f"{col}-{idx}", cn="kanban-card",
            span(cn="kanban-card-title", title),
            div(cn="kanban-card-actions",
                button(cn="kanban-move", oc=lambda: move(col, idx, -1), "←")
                    if col_i > 0 else None,
                button(cn="kanban-move", oc=lambda: move(col, idx, 1), "→")
                    if col_i < len(COLUMNS) - 1 else None,
            ),
        )

    def render_column(col):
        return div(key=col, cn="kanban-column",
            h2(cn="kanban-column-title", col),
            ul(cn="kanban-list",
                *[render_card(col, idx, title)
                  for idx, title in enumerate(board[col])]),
            div(cn="kanban-add",
                input(cn="kanban-input", value=drafts[col],
                    oh=lambda e: set_draft(col, e.target.value),
                    ph=f"Add to {col}..."),
                button(cn="kanban-add-btn", oc=lambda: add(col), "Add"),
            ),
        )

    return div(cn="kanban",
        h1(cn="kanban-heading", "KanbanLite"),
        div(cn="kanban-board",
            *[render_column(col) for col in COLUMNS]),
    )
```
