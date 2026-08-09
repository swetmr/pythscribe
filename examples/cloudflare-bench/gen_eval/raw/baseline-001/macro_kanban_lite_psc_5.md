<!-- exp=baseline-001 task=macro_kanban_lite cond=psc sample=5 model=claude-opus-4-8 cost_usd=0.12494000000000001 -->
```python
"use client"
R*

columns = ["Todo", "Doing", "Done"]

@c
def KanbanLite():
    board, set_board = us({
        "Todo": ["Write spec", "Sketch UI"],
        "Doing": ["Wire state"],
        "Done": ["Set up repo"],
    })
    drafts, set_drafts = us({"Todo": "", "Doing": "", "Done": ""})

    def move(col_i, card_i, delta):
        target_i = col_i + delta
        src = columns[col_i]
        dst = columns[target_i]
        new_board = {k: [c for c in v] for k, v in board.items()}
        card = new_board[src].pop(card_i)
        new_board[dst].append(card)
        set_board(new_board)

    def add(col):
        title = drafts[col].strip()
        if title:
            set_board({**board, col: [*board[col], title]})
            set_drafts({**drafts, col: ""})

    def render_card(col_i, card_i, title):
        return div(cn="kanban-card", key=card_i,
            span(cn="kanban-card-title", title),
            div(cn="kanban-card-actions",
                (button(cn="kanban-move", oc=lambda: move(col_i, card_i, -1), "\u2190")
                 if col_i > 0 else None),
                (button(cn="kanban-move", oc=lambda: move(col_i, card_i, 1), "\u2192")
                 if col_i < len(columns) - 1 else None),
            ),
        )

    def render_column(col_i, col):
        cards = board[col]
        return div(cn="kanban-column", key=col,
            h3(cn="kanban-column-title", col),
            div(cn="kanban-card-list",
                *[render_card(col_i, i, t) for i, t in enumerate(cards)]),
            div(cn="kanban-add",
                input(cn="kanban-input", value=drafts[col],
                      ph=f"Add to {col}...",
                      oh=lambda e, c=col: set_drafts({**drafts, c: e.target.value})),
                button(cn="kanban-add-btn", oc=lambda c=col: add(c), "Add"),
            ),
        )

    return div(cn="kanban-lite",
        h2(cn="kanban-heading", "KanbanLite"),
        div(cn="kanban-board",
            *[render_column(i, col) for i, col in enumerate(columns)]),
    )
```
