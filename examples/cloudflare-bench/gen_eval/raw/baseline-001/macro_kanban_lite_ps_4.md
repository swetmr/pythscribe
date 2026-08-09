<!-- exp=baseline-001 task=macro_kanban_lite cond=ps sample=4 model=claude-opus-4-8 cost_usd=0.037882 -->
```python
"use client"

from pyths.react import component, use_state

INITIAL_BOARD = {
    "todo": ["Draft spec", "Buy groceries", "Sketch logo"],
    "doing": ["Refactor parser", "Review PR #112"],
    "done": ["Set up CI", "Write README"],
}

COLUMNS = [
    ("todo", "Todo"),
    ("doing", "Doing"),
    ("done", "Done"),
]


def column_order():
    return [key for key, _ in COLUMNS]


def adjacent(key, delta):
    order = column_order()
    idx = order.index(key) + delta
    if idx < 0 or idx >= len(order):
        return None
    return order[idx]


@component
def KanbanLite():
    board, set_board = use_state(INITIAL_BOARD)
    drafts, set_drafts = use_state({"todo": "", "doing": "", "done": ""})

    def set_draft(col, value):
        set_drafts({**drafts, col: value})

    def move_card(col, index, delta):
        target = adjacent(col, delta)
        if target is None:
            return
        card = board[col][index]
        source_cards = [c for i, c in enumerate(board[col]) if i != index]
        target_cards = [*board[target], card]
        set_board({**board, col: source_cards, target: target_cards})

    def add_card(col):
        title = drafts[col].strip()
        if not title:
            return
        set_board({**board, col: [*board[col], title]})
        set_draft(col, "")

    def render_card(col, index, title):
        left = adjacent(col, -1)
        right = adjacent(col, 1)
        return li(key=f"{col}-{index}", class_name="kanban-card",
            span(class_name="kanban-card-title", title),
            div(class_name="kanban-card-actions",
                button(class_name="kanban-move left",
                       on_click=lambda: move_card(col, index, -1),
                       "◀") if left is not None else None,
                button(class_name="kanban-move right",
                       on_click=lambda: move_card(col, index, 1),
                       "▶") if right is not None else None,
            ),
        )

    def render_column(key, label):
        cards = board[key]
        return section(key=key, class_name=f"kanban-column {key}",
            h2(class_name="kanban-column-title", f"{label} ({len(cards)})"),
            ul(class_name="kanban-list",
                *[render_card(key, i, title) for i, title in enumerate(cards)]),
            div(class_name="kanban-add",
                input(value=drafts[key], class_name="kanban-input",
                      placeholder=f"Add to {label}...",
                      on_change=lambda e: set_draft(key, e.target.value),
                      on_key_down=lambda e: add_card(key) if e.key == "Enter" else None),
                button(class_name="kanban-add-btn",
                       on_click=lambda: add_card(key), "Add"),
            ),
        )

    return div(class_name="kanban",
        h1(class_name="kanban-heading", "KanbanLite"),
        div(class_name="kanban-board",
            *[render_column(key, label) for key, label in COLUMNS]),
    )
```
