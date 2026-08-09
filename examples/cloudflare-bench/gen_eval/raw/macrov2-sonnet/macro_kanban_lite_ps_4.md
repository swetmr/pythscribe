<!-- exp=macrov2-sonnet task=macro_kanban_lite cond=ps sample=4 model=claude-sonnet-5 cost_usd=0.0887916 -->
```python
"use client"

from pyths.react import component, use_state

COLUMNS = ["Todo", "Doing", "Done"]

INITIAL_BOARD = {
    "Todo": [
        {"id": 1, "title": "Design mockups"},
        {"id": 2, "title": "Research competitors"},
    ],
    "Doing": [
        {"id": 3, "title": "Build login page"},
    ],
    "Done": [
        {"id": 4, "title": "Set up repo"},
    ],
}

@component
def KanbanLite():
    board, set_board = use_state(INITIAL_BOARD)
    inputs, set_inputs = use_state({"Todo": "", "Doing": "", "Done": ""})
    next_id, set_next_id = use_state(5)

    def move_card(col_name, card_id, direction):
        idx = COLUMNS.index(col_name)
        target_idx = idx + direction
        if target_idx < 0 or target_idx >= len(COLUMNS):
            return
        target_name = COLUMNS[target_idx]
        card = None
        for c in board[col_name]:
            if c["id"] == card_id:
                card = c
        new_board = {**board}
        new_board[col_name] = [c for c in board[col_name] if c["id"] != card_id]
        new_board[target_name] = [*board[target_name], card]
        set_board(new_board)

    def add_card(col_name):
        title = inputs[col_name].strip()
        if not title:
            return
        new_card = {"id": next_id, "title": title}
        new_board = {**board, col_name: [*board[col_name], new_card]}
        set_board(new_board)
        set_inputs({**inputs, col_name: ""})
        set_next_id(next_id + 1)

    def set_input(col_name, value):
        set_inputs({**inputs, col_name: value})

    return div(class_name="kanban-board",
        *[
            div(key=col_name, class_name="kanban-column",
                h2(class_name="column-title", col_name),
                ul(class_name="kanban-cards",
                    *[
                        li(key=card["id"], class_name="kanban-card",
                            span(class_name="card-title", card["title"]),
                            div(class_name="card-actions",
                                (COLUMNS.index(col_name) > 0) and button(
                                    class_name="move-btn move-left",
                                    on_click=lambda: move_card(col_name, card["id"], -1),
                                    "\u25c0",
                                ),
                                (COLUMNS.index(col_name) < len(COLUMNS) - 1) and button(
                                    class_name="move-btn move-right",
                                    on_click=lambda: move_card(col_name, card["id"], 1),
                                    "\u25b6",
                                ),
                            ),
                        )
                        for card in board[col_name]
                    ]
                ),
                div(class_name="add-card-form",
                    input(
                        class_name="add-card-input",
                        value=inputs[col_name],
                        on_change=lambda e: set_input(col_name, e.target.value),
                        placeholder=f"New {col_name} card...",
                    ),
                    button(
                        class_name="add-card-btn",
                        on_click=lambda: add_card(col_name),
                        "Add",
                    ),
                ),
            )
            for col_name in COLUMNS
        ]
    )
```
