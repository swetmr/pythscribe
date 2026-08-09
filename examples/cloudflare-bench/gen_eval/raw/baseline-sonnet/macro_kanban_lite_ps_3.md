<!-- exp=baseline-sonnet task=macro_kanban_lite cond=ps sample=3 model=claude-sonnet-5 cost_usd=0.026931899999999998 -->
```python
"use client"

from pyths.react import component, use_state

INITIAL_BOARD = {
    "todo": ["Write spec", "Research options", "Set up repo"],
    "doing": ["Build prototype"],
    "done": ["Kickoff meeting"],
}

COLUMNS = [
    {"key": "todo", "label": "Todo"},
    {"key": "doing", "label": "Doing"},
    {"key": "done", "label": "Done"},
]


def next_column(key):
    if key == "todo":
        return "doing"
    if key == "doing":
        return "done"
    return None


def prev_column(key):
    if key == "doing":
        return "todo"
    if key == "done":
        return "doing"
    return None


@component
def KanbanLite():
    board, set_board = use_state(INITIAL_BOARD)
    drafts, set_drafts = use_state({"todo": "", "doing": "", "done": ""})

    def move_card(from_key, to_key, index):
        if to_key is None:
            return
        card = board[from_key][index]
        new_from = [c for i, c in enumerate(board[from_key]) if i != index]
        new_to = [*board[to_key], card]
        set_board({**board, from_key: new_from, to_key: new_to})

    def set_draft(col_key, value):
        set_drafts({**drafts, col_key: value})

    def add_card(col_key):
        text = drafts[col_key].strip()
        if not text:
            return
        set_board({**board, col_key: [*board[col_key], text]})
        set_draft(col_key, "")

    def render_card(col_key, title, index):
        left_key = prev_column(col_key)
        right_key = next_column(col_key)
        return div(key=f"{col_key}-{index}-{title}", class_name="kanban-card",
            span(class_name="kanban-card-title", title),
            div(class_name="kanban-card-actions",
                left_key and button(class_name="kanban-move-btn",
                                     on_click=lambda: move_card(col_key, left_key, index),
                                     "<"),
                right_key and button(class_name="kanban-move-btn",
                                      on_click=lambda: move_card(col_key, right_key, index),
                                      ">"),
            ),
        )

    def render_column(col):
        col_key = col["key"]
        cards = board[col_key]
        return div(key=col_key, class_name="kanban-column",
            h2(class_name="kanban-column-title", col["label"]),
            div(class_name="kanban-card-list",
                *[render_card(col_key, title, i) for i, title in enumerate(cards)],
            ),
            div(class_name="kanban-add-row",
                input(class_name="kanban-add-input",
                      value=drafts[col_key],
                      placeholder="New card...",
                      on_change=lambda e: set_draft(col_key, e.target.value),
                      on_key_down=lambda e: add_card(col_key) if e.key == "Enter" else None),
                button(class_name="kanban-add-btn",
                       on_click=lambda: add_card(col_key),
                       "Add"),
            ),
        )

    return div(class_name="kanban-board",
        *[render_column(col) for col in COLUMNS],
    )
```
