<!-- exp=macrov2-opus task=macro_kanban_lite cond=ps sample=4 model=claude-opus-4-8 cost_usd=0.036244 -->
```python
"use client"

from pyths.react import component, use_state

COLUMNS = ["Todo", "Doing", "Done"]

INITIAL_BOARD = {
    "Todo": ["Write spec", "Sketch UI"],
    "Doing": ["Wire up state"],
    "Done": ["Set up repo"],
}


@component
def KanbanLite():
    board, set_board = use_state(INITIAL_BOARD)
    drafts, set_drafts = use_state({"Todo": "", "Doing": "", "Done": ""})

    def move(col, index, direction):
        col_i = COLUMNS.index(col)
        target = COLUMNS[col_i + direction]
        next_board = {c: [card for card in board[c]] for c in COLUMNS}
        card = next_board[col].pop(index)
        next_board[target].append(card)
        set_board(next_board)

    def set_draft(col, value):
        set_drafts({**drafts, col: value})

    def add(col):
        title = drafts[col].strip()
        if title:
            next_board = {c: [card for card in board[c]] for c in COLUMNS}
            next_board[col].append(title)
            set_board(next_board)
            set_drafts({**drafts, col: ""})

    return div(class_name="kanban",
        h2(class_name="kanban-title", "KanbanLite"),
        div(class_name="kanban-columns",
            *[Column(
                key=col,
                col=col,
                cards=board[col],
                draft=drafts[col],
                on_move=move,
                on_draft=set_draft,
                on_add=add,
            ) for col in COLUMNS],
        ),
    )


@component
def Column(col, cards, draft, on_move, on_draft, on_add):
    col_i = COLUMNS.index(col)
    can_left = col_i > 0
    can_right = col_i < len(COLUMNS) - 1

    return div(class_name="kanban-column",
        h3(class_name="kanban-column-title", col),
        ul(class_name="kanban-cards",
            *[li(key=f"{col}-{i}", class_name="kanban-card",
                span(class_name="kanban-card-title", title),
                div(class_name="kanban-card-actions",
                    button(class_name="kanban-move left",
                           on_click=lambda: on_move(col, i, -1),
                           "\u2190") if can_left else None,
                    button(class_name="kanban-move right",
                           on_click=lambda: on_move(col, i, 1),
                           "\u2192") if can_right else None,
                ),
            ) for i, title in enumerate(cards)],
        ),
        div(class_name="kanban-add",
            input(class_name="kanban-input", value=draft,
                  placeholder=f"Add to {col}...",
                  on_change=lambda e: on_draft(col, e.target.value)),
            button(class_name="kanban-add-btn",
                   on_click=lambda: on_add(col), "Add"),
        ),
    )
```
