<!-- exp=baseline-001 task=macro_kanban_lite cond=ps sample=2 model=claude-opus-4-8 cost_usd=0.037707 -->
```python
"use client"

from pyths.react import component, use_state

INITIAL_BOARD = {
    "Todo": ["Draft spec", "Sketch UI", "Collect fixtures"],
    "Doing": ["Wire up state"],
    "Done": ["Set up repo"],
}

COLUMNS = ["Todo", "Doing", "Done"]


def move_card(board, column, index, delta):
    col_pos = COLUMNS.index(column)
    target_pos = col_pos + delta
    if target_pos < 0 or target_pos >= len(COLUMNS):
        return board
    target = COLUMNS[target_pos]
    next_board = {key: [card for card in cards] for key, cards in board.items()}
    card = next_board[column].pop(index)
    next_board[target].append(card)
    return next_board


def add_card(board, column, title):
    clean = title.strip()
    if not clean:
        return board
    next_board = {key: [card for card in cards] for key, cards in board.items()}
    next_board[column].append(clean)
    return next_board


@component
def KanbanLite():
    board, set_board = use_state(INITIAL_BOARD)
    drafts, set_drafts = use_state({"Todo": "", "Doing": "", "Done": ""})

    def set_draft(column, value):
        set_drafts({**drafts, column: value})

    def handle_add(column):
        set_board(add_card(board, column, drafts[column]))
        set_draft(column, "")

    def handle_move(column, index, delta):
        set_board(move_card(board, column, index, delta))

    return div(class_name="kanban",
        h1(class_name="kanban-title", "KanbanLite"),
        div(class_name="kanban-columns",
            *[div(key=column, class_name="kanban-column",
                h2(class_name="column-heading", column),
                ul(class_name="card-list",
                    *[li(key=f"{column}-{i}", class_name="card",
                        span(class_name="card-title", title),
                        div(class_name="card-actions",
                            (button(class_name="move-left",
                                    on_click=lambda col=column, idx=i: handle_move(col, idx, -1),
                                    "←")
                             if col_pos > 0 else None),
                            (button(class_name="move-right",
                                    on_click=lambda col=column, idx=i: handle_move(col, idx, 1),
                                    "→")
                             if col_pos < len(COLUMNS) - 1 else None),
                        ),
                    ) for i, title in enumerate(board[column])]),
                div(class_name="add-row",
                    input(class_name="add-input", value=drafts[column],
                          placeholder=f"Add to {column}...",
                          on_change=lambda e, col=column: set_draft(col, e.target.value),
                          on_key_down=lambda e, col=column: handle_add(col) if e.key == "Enter" else None),
                    button(class_name="add-button",
                           on_click=lambda col=column: handle_add(col), "Add"),
                ),
            ) for col_pos, column in enumerate(COLUMNS)],
        ),
    )
```
