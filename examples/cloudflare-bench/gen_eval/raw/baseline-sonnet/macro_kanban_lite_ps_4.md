<!-- exp=baseline-sonnet task=macro_kanban_lite cond=ps sample=4 model=claude-sonnet-5 cost_usd=0.026076900000000004 -->
```python
"use client"

from pyths.react import component, use_state

INITIAL_BOARD = {
    "todo": ["Write proposal", "Research vendors"],
    "doing": ["Design mockups"],
    "done": ["Kickoff meeting"],
}

COLUMNS = [
    {"key": "todo", "title": "Todo"},
    {"key": "doing", "title": "Doing"},
    {"key": "done", "title": "Done"},
]


def next_key(key):
    if key == "todo":
        return "doing"
    if key == "doing":
        return "done"
    return None


def prev_key(key):
    if key == "done":
        return "doing"
    if key == "doing":
        return "todo"
    return None


@component
def KanbanLite():
    board, set_board = use_state(INITIAL_BOARD)
    drafts, set_drafts = use_state({"todo": "", "doing": "", "done": ""})

    def move_card(from_key, index, to_key):
        if to_key is None:
            return
        card = board[from_key][index]
        new_from = [c for i, c in enumerate(board[from_key]) if i != index]
        new_to = [*board[to_key], card]
        set_board({**board, from_key: new_from, to_key: new_to})

    def update_draft(key, value):
        set_drafts({**drafts, key: value})

    def add_card(key):
        text = drafts[key].strip()
        if not text:
            return
        set_board({**board, key: [*board[key], text]})
        set_drafts({**drafts, key: ""})

    def render_column(col):
        key = col["key"]
        cards = board[key]
        left = prev_key(key)
        right = next_key(key)
        return div(class_name="kanban-column", key=key,
            h3(class_name="kanban-column-title", col["title"]),
            ul(class_name="kanban-card-list",
                *[li(class_name="kanban-card", key=f"{key}-{i}",
                    left and button(class_name="kanban-move-btn",
                                     on_click=lambda i=i: move_card(key, i, left), "<"),
                    span(class_name="kanban-card-title", title),
                    right and button(class_name="kanban-move-btn",
                                      on_click=lambda i=i: move_card(key, i, right), ">"),
                ) for i, title in enumerate(cards)]
            ),
            div(class_name="kanban-add-row",
                input(class_name="kanban-add-input", value=drafts[key],
                      placeholder="New card...",
                      on_change=lambda e, key=key: update_draft(key, e.target.value)),
                button(class_name="kanban-add-btn",
                       on_click=lambda key=key: add_card(key), "Add"),
            ),
        )

    return div(class_name="kanban-board",
        *[render_column(col) for col in COLUMNS]
    )
```
