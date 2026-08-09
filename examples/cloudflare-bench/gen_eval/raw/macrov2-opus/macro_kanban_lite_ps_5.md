<!-- exp=macrov2-opus task=macro_kanban_lite cond=ps sample=5 model=claude-opus-4-8 cost_usd=0.034344 -->
```python
"use client"

from pyths.react import component, use_state

COLUMNS = ["Todo", "Doing", "Done"]

INITIAL_BOARD = {
    "Todo": ["Write spec", "Sketch UI"],
    "Doing": ["Build parser"],
    "Done": ["Set up repo"],
}


@component
def KanbanLite():
    board, set_board = use_state(INITIAL_BOARD)
    drafts, set_drafts = use_state({col: "" for col in COLUMNS})

    def move(col, idx, direction):
        pos = COLUMNS.index(col)
        target = COLUMNS[pos + direction]
        next_board = {c: [t for t in board[c]] for c in COLUMNS}
        card = next_board[col].pop(idx)
        next_board[target].append(card)
        set_board(next_board)

    def set_draft(col, value):
        set_drafts({**drafts, col: value})

    def add(col):
        title = drafts[col].strip()
        if title:
            next_board = {c: [t for t in board[c]] for c in COLUMNS}
            next_board[col].append(title)
            set_board(next_board)
            set_draft(col, "")

    def render_card(col, idx, title):
        pos = COLUMNS.index(col)
        return div(key=f"{col}-{idx}", class_name="kanban-card",
            span(class_name="kanban-card-title", title),
            div(class_name="kanban-card-actions",
                button(class_name="kanban-move left",
                       on_click=lambda: move(col, idx, -1),
                       "◀") if pos > 0 else None,
                button(class_name="kanban-move right",
                       on_click=lambda: move(col, idx, 1),
                       "▶") if pos < len(COLUMNS) - 1 else None,
            ),
        )

    def render_column(col):
        return div(key=col, class_name="kanban-column",
            h2(class_name="kanban-column-title", col),
            div(class_name="kanban-cards",
                *[render_card(col, idx, title)
                  for idx, title in enumerate(board[col])]),
            div(class_name="kanban-add",
                input(value=drafts[col],
                      placeholder=f"Add to {col}...",
                      on_change=lambda e: set_draft(col, e.target.value),
                      on_key_down=lambda e: add(col) if e.key == "Enter" else None),
                button(class_name="kanban-add-btn",
                       on_click=lambda: add(col),
                       "Add"),
            ),
        )

    return div(class_name="kanban-board",
        h1(class_name="kanban-heading", "KanbanLite"),
        div(class_name="kanban-columns",
            *[render_column(col) for col in COLUMNS]),
    )
```
