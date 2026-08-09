<!-- exp=baseline-001 task=macro_kanban_lite cond=ps sample=3 model=claude-opus-4-8 cost_usd=0.038607 -->
```python
"use client"

from pyths.react import component, use_state

COLUMNS = ["Todo", "Doing", "Done"]

INITIAL_BOARD = {
    "Todo": ["Write spec", "Sketch UI"],
    "Doing": ["Build parser"],
    "Done": ["Set up repo"],
}


def move_card(board, from_col, index, delta):
    col_idx = COLUMNS.index(from_col)
    target_idx = col_idx + delta
    if target_idx < 0 or target_idx >= len(COLUMNS):
        return board
    to_col = COLUMNS[target_idx]
    new_board = {c: [t for t in board[c]] for c in COLUMNS}
    card = new_board[from_col].pop(index)
    new_board[to_col].append(card)
    return new_board


def add_card(board, col, title):
    new_board = {c: [t for t in board[c]] for c in COLUMNS}
    new_board[col].append(title)
    return new_board


@component
def KanbanLite():
    board, set_board = use_state(INITIAL_BOARD)
    drafts, set_drafts = use_state({c: "" for c in COLUMNS})

    def set_draft(col, value):
        set_drafts({**drafts, col: value})

    def handle_add(col):
        title = drafts[col].strip()
        if title:
            set_board(add_card(board, col, title))
            set_draft(col, "")

    def handle_move(col, index, delta):
        set_board(move_card(board, col, index, delta))

    def render_card(col, index, title):
        col_idx = COLUMNS.index(col)
        can_left = col_idx > 0
        can_right = col_idx < len(COLUMNS) - 1
        return li(key=f"{col}-{index}-{title}", class_name="kanban-card",
            span(class_name="kanban-card-title", title),
            div(class_name="kanban-card-actions",
                button(class_name="kanban-move",
                       on_click=lambda: handle_move(col, index, -1),
                       "←") if can_left else None,
                button(class_name="kanban-move",
                       on_click=lambda: handle_move(col, index, 1),
                       "→") if can_right else None,
            ),
        )

    def render_column(col):
        return div(key=col, class_name="kanban-column",
            h2(class_name="kanban-column-title", col),
            ul(class_name="kanban-card-list",
                *[render_card(col, i, t) for i, t in enumerate(board[col])]),
            div(class_name="kanban-add",
                input(class_name="kanban-input", value=drafts[col],
                      placeholder=f"Add to {col}...",
                      on_change=lambda e: set_draft(col, e.target.value),
                      on_key_down=lambda e: handle_add(col) if e.key == "Enter" else None),
                button(class_name="kanban-add-btn",
                       on_click=lambda: handle_add(col), "Add"),
            ),
        )

    return div(class_name="kanban-lite",
        h1(class_name="kanban-heading", "KanbanLite"),
        div(class_name="kanban-board",
            *[render_column(c) for c in COLUMNS]),
    )
```
