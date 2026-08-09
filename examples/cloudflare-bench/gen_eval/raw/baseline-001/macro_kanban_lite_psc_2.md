<!-- exp=baseline-001 task=macro_kanban_lite cond=psc sample=2 model=claude-opus-4-8 cost_usd=0.037122 -->
```python
"use client"
R*

COLUMNS = ["todo", "doing", "done"]
LABELS = {"todo": "Todo", "doing": "Doing", "done": "Done"}

@c
def KanbanLite():
    board, set_board = us({
        "todo": ["Write spec", "Sketch UI"],
        "doing": ["Build parser"],
        "done": ["Set up repo"],
    })
    drafts, set_drafts = us({"todo": "", "doing": "", "done": ""})

    def move(col, i, delta):
        idx = COLUMNS.index(col)
        target = COLUMNS[idx + delta]
        next_board = {k: [c for c in v] for k, v in board.items()}
        card = next_board[col].pop(i)
        next_board[target].append(card)
        set_board(next_board)

    def add(col):
        title = drafts[col].strip()
        if title:
            set_board({**board, col: [*board[col], title]})
            set_drafts({**drafts, col: ""})

    def render_card(col, i, title):
        idx = COLUMNS.index(col)
        return li(key=f"{col}-{i}", cn="kanban-card",
            span(cn="kanban-card-title", title),
            div(cn="kanban-card-actions",
                button(cn="kanban-move", oc=lambda: move(col, i, -1), "←")
                    if idx > 0 else None,
                button(cn="kanban-move", oc=lambda: move(col, i, 1), "→")
                    if idx < len(COLUMNS) - 1 else None,
            ),
        )

    def render_column(col):
        cards = board[col]
        return div(key=col, cn="kanban-column",
            h2(cn="kanban-column-title", LABELS[col]),
            ul(cn="kanban-list",
                *[render_card(col, i, t) for i, t in enumerate(cards)]),
            div(cn="kanban-add",
                input(cn="kanban-input", value=drafts[col],
                    ph="New card...",
                    oh=lambda e, col=col: set_drafts({**drafts, col: e.target.value})),
                button(cn="kanban-add-btn", oc=lambda col=col: add(col), "Add"),
            ),
        )

    return div(cn="kanban",
        h1(cn="kanban-heading", "KanbanLite"),
        div(cn="kanban-board",
            *[render_column(col) for col in COLUMNS]),
    )
```
