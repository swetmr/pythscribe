<!-- exp=macrov2-sonnet task=macro_kanban_lite cond=ps sample=1 model=claude-sonnet-5 cost_usd=0.044967599999999996 -->
```python
"use client"

from pyths.react import component, use_state

COLUMNS = ["Todo", "Doing", "Done"]

INITIAL_BOARD = {
    "Todo": ["Design landing page", "Write onboarding docs"],
    "Doing": ["Fix login bug"],
    "Done": ["Set up CI pipeline"],
}


def move_card(board, col, idx, direction):
    new_board = {k: [c for c in v] for k, v in board.items()}
    target_idx = COLUMNS.index(col) + direction
    target_col = COLUMNS[target_idx]
    title = new_board[col].pop(idx)
    new_board[target_col].append(title)
    return new_board


@component
def KanbanLite():
    board, set_board = use_state(INITIAL_BOARD)
    drafts, set_drafts = use_state({"Todo": "", "Doing": "", "Done": ""})

    def add_card(col):
        title = drafts[col].strip()
        if title:
            new_board = {k: [c for c in v] for k, v in board.items()}
            new_board[col].append(title)
            set_board(new_board)
            set_drafts({**drafts, col: ""})

    def move(col, idx, direction):
        set_board(move_card(board, col, idx, direction))

    def render_column(col):
        col_idx = COLUMNS.index(col)
        can_go_left = col_idx > 0
        can_go_right = col_idx < len(COLUMNS) - 1

        def render_card(title, idx):
            return div(class_name="kanban-card", key=idx,
                button(class_name="kanban-move-btn", on_click=lambda: move(col, idx, -1), "←") if can_go_left else None,
                span(class_name="kanban-card-title", title),
                button(class_name="kanban-move-btn", on_click=lambda: move(col, idx, 1), "→") if can_go_right else None,
            )

        return div(class_name="kanban-column", key=col,
            h3(class_name="kanban-column-title", col),
            div(class_name="kanban-card-list",
                *[render_card(title, idx) for idx, title in enumerate(board[col])],
            ),
            div(class_name="kanban-add-row",
                input(
                    class_name="kanban-add-input",
                    value=drafts[col],
                    placeholder=f"New {col} card...",
                    on_change=lambda e: set_drafts({**drafts, col: e.target.value}),
                    on_key_down=lambda e: add_card(col) if e.key == "Enter" else None,
                ),
                button(class_name="kanban-add-btn", on_click=lambda: add_card(col), "Add"),
            ),
        )

    return div(class_name="kanban-board",
        *[render_column(col) for col in COLUMNS],
    )
```
