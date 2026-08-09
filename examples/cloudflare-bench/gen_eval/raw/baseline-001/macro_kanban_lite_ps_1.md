<!-- exp=baseline-001 task=macro_kanban_lite cond=ps sample=1 model=claude-opus-4-8 cost_usd=0.0700305 -->
```python
"use client"

from pyths.react import component, use_state

COLUMNS = ["Todo", "Doing", "Done"]

INITIAL_BOARD = {
    "Todo": ["Write spec", "Sketch UI"],
    "Doing": ["Build API"],
    "Done": ["Set up repo"],
}


def move_card(board, col_index, card_index, direction):
    target_index = col_index + direction
    if target_index < 0 or target_index >= len(COLUMNS):
        return board
    from_col = COLUMNS[col_index]
    to_col = COLUMNS[target_index]
    next_board = {k: [c for c in v] for k, v in board.items()}
    card = next_board[from_col].pop(card_index)
    next_board[to_col].append(card)
    return next_board


def add_card(board, col, title):
    text = title.strip()
    if not text:
        return board
    next_board = {k: [c for c in v] for k, v in board.items()}
    next_board[col].append(text)
    return next_board


@component
def KanbanLite():
    board, set_board = use_state(INITIAL_BOARD)
    drafts, set_drafts = use_state({col: "" for col in COLUMNS})

    def handle_move(col_index, card_index, direction):
        set_board(move_card(board, col_index, card_index, direction))

    def handle_draft(col, value):
        set_drafts({**drafts, col: value})

    def handle_add(col):
        set_board(add_card(board, col, drafts[col]))
        set_drafts({**drafts, col: ""})

    return div(class_name="kanban",
        h2(class_name="kanban-title", "KanbanLite"),
        div(class_name="kanban-columns",
            *[div(key=col, class_name="kanban-column",
                h3(class_name="column-header", col),
                ul(class_name="card-list",
                    *[li(key=f"{col}-{i}", class_name="card",
                        span(class_name="card-title", title),
                        div(class_name="card-actions",
                            (button(class_name="move-left",
                                    on_click=lambda ci=col_index, cardi=i: handle_move(ci, cardi, -1),
                                    "←")
                                if col_index > 0 else None),
                            (button(class_name="move-right",
                                    on_click=lambda ci=col_index, cardi=i: handle_move(ci, cardi, 1),
                                    "→")
                                if col_index < len(COLUMNS) - 1 else None),
                        ),
                    ) for i, title in enumerate(board[col])]),
                div(class_name="add-card",
                    input(class_name="add-input", value=drafts[col],
                          placeholder=f"Add to {col}...",
                          on_change=lambda e, c=col: handle_draft(c, e.target.value),
                          on_key_down=lambda e, c=col: handle_add(c) if e.key == "Enter" else None),
                    button(class_name="add-button",
                           on_click=lambda c=col: handle_add(c), "Add"),
                ),
            ) for col_index, col in enumerate(COLUMNS)]),
    )
```
