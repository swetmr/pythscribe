<!-- exp=macrov2-opus task=macro_kanban_lite cond=ps sample=1 model=claude-opus-4-8 cost_usd=0.069629 -->
```python
"use client"

from pyths.react import component, use_state

COLUMNS = ["Todo", "Doing", "Done"]

INITIAL_BOARD = {
    "Todo": ["Write spec", "Sketch UI", "Set up repo"],
    "Doing": ["Build API", "Draft tests"],
    "Done": ["Kickoff meeting"],
}


@component
def KanbanLite():
    board, set_board = use_state(INITIAL_BOARD)
    drafts, set_drafts = use_state({"Todo": "", "Doing": "", "Done": ""})

    def move(col_index, card_index, direction):
        target_index = col_index + direction
        from_col = COLUMNS[col_index]
        to_col = COLUMNS[target_index]
        next_board = {k: [c for c in v] for k, v in board.items()}
        card = next_board[from_col].pop(card_index)
        next_board[to_col].append(card)
        set_board(next_board)

    def add(col):
        title = drafts[col].strip()
        if title:
            next_board = {k: [c for c in v] for k, v in board.items()}
            next_board[col].append(title)
            set_board(next_board)
            set_drafts({**drafts, col: ""})

    def set_draft(col, value):
        set_drafts({**drafts, col: value})

    return div(class_name="kanban",
        h2(class_name="kanban-title", "KanbanLite"),
        div(class_name="kanban-columns",
            *[div(key=col, class_name="kanban-column",
                h3(class_name="kanban-column-title", col),
                ul(class_name="kanban-cards",
                    *[li(key=f"{col}-{j}", class_name="kanban-card",
                        span(class_name="kanban-card-title", title),
                        div(class_name="kanban-card-actions",
                            button(class_name="kanban-move kanban-move-left",
                                   on_click=lambda: move(i, j, -1),
                                   "\u2190") if i > 0 else None,
                            button(class_name="kanban-move kanban-move-right",
                                   on_click=lambda: move(i, j, 1),
                                   "\u2192") if i < len(COLUMNS) - 1 else None,
                        ),
                    ) for j, title in enumerate(board[col])]),
                div(class_name="kanban-add",
                    input(class_name="kanban-add-input",
                          value=drafts[col],
                          placeholder=f"Add to {col}...",
                          on_change=lambda e: set_draft(col, e.target.value),
                          on_key_down=lambda e: add(col) if e.key == "Enter" else None),
                    button(class_name="kanban-add-button",
                           on_click=lambda: add(col), "Add"),
                ),
            ) for i, col in enumerate(COLUMNS)]),
    )
```
