<!-- exp=baseline-sonnet task=macro_kanban_lite cond=ps sample=1 model=claude-sonnet-5 cost_usd=0.044435999999999996 -->
```python
"use client"

from pyths.react import component, use_state

COLUMNS = ["Todo", "Doing", "Done"]

INITIAL_BOARD = {
    "Todo": ["Write proposal", "Research vendors"],
    "Doing": ["Design mockups"],
    "Done": ["Kickoff meeting"],
}

@component
def KanbanLite():
    board, set_board = use_state({**INITIAL_BOARD})
    drafts, set_drafts = use_state({col: "" for col in COLUMNS})

    def set_draft(col, value):
        set_drafts({**drafts, col: value})

    def add_card(col):
        title = drafts[col].strip()
        if not title:
            return
        set_board({**board, col: [*board[col], title]})
        set_drafts({**drafts, col: ""})

    def move_card(col, index, direction):
        col_i = COLUMNS.index(col)
        target_i = col_i + direction
        if target_i < 0 or target_i >= len(COLUMNS):
            return
        target_col = COLUMNS[target_i]
        title = board[col][index]
        new_source = [t for i, t in enumerate(board[col]) if i != index]
        new_target = [*board[target_col], title]
        set_board({**board, col: new_source, target_col: new_target})

    def render_card(col, index, title):
        col_i = COLUMNS.index(col)
        can_move_left = col_i > 0
        can_move_right = col_i < len(COLUMNS) - 1
        return div(key=index, class_name="kanban-card",
            span(class_name="kanban-card-title", title),
            div(class_name="kanban-card-actions",
                can_move_left and button(
                    class_name="kanban-move-left",
                    on_click=lambda: move_card(col, index, -1),
                    "<",
                ),
                can_move_right and button(
                    class_name="kanban-move-right",
                    on_click=lambda: move_card(col, index, 1),
                    ">",
                ),
            ),
        )

    def render_column(col):
        return div(key=col, class_name="kanban-column",
            h3(class_name="kanban-column-title", col),
            div(class_name="kanban-card-list",
                *[render_card(col, i, title) for i, title in enumerate(board[col])],
            ),
            div(class_name="kanban-add-row",
                input(
                    class_name="kanban-add-input",
                    value=drafts[col],
                    placeholder=f"Add to {col}...",
                    on_change=lambda e: set_draft(col, e.target.value),
                    on_key_down=lambda e: add_card(col) if e.key == "Enter" else None,
                ),
                button(
                    class_name="kanban-add-button",
                    on_click=lambda: add_card(col),
                    "Add",
                ),
            ),
        )

    return div(class_name="kanban-board",
        *[render_column(col) for col in COLUMNS],
    )
```
