<!-- exp=macrov2-sonnet task=macro_kanban_lite cond=ps sample=5 model=claude-sonnet-5 cost_usd=0.0240216 -->
```python
"use client"

from pyths.react import component, use_state

INITIAL_BOARD = {
    "todo": ["Design landing page", "Write onboarding docs"],
    "doing": ["Fix login bug"],
    "done": ["Set up CI pipeline"],
}

COLUMNS = [
    {"key": "todo", "label": "Todo"},
    {"key": "doing", "label": "Doing"},
    {"key": "done", "label": "Done"},
]

def next_key(key):
    if key == "todo":
        return "doing"
    if key == "doing":
        return "done"
    return None

def prev_key(key):
    if key == "doing":
        return "todo"
    if key == "done":
        return "doing"
    return None

@component
def KanbanColumn(col_key, label, cards, on_move, on_add):
    text, set_text = use_state("")

    def handle_add():
        if text.strip():
            on_add(col_key, text.strip())
            set_text("")

    left = prev_key(col_key)
    right = next_key(col_key)

    return div(class_name="kanban-column",
        h3(class_name="kanban-column-title", label),
        ul(class_name="kanban-card-list",
            *[li(key=card, class_name="kanban-card",
                left and button(on_click=lambda: on_move(col_key, left, card), class_name="kanban-move-left", "<"),
                span(class_name="kanban-card-title", card),
                right and button(on_click=lambda: on_move(col_key, right, card), class_name="kanban-move-right", ">"),
              ) for card in cards]
        ),
        div(class_name="kanban-add-row",
            input(value=text, on_change=lambda e: set_text(e.target.value),
                  placeholder=f"New {label} card...", class_name="kanban-add-input"),
            button(on_click=lambda: handle_add(), class_name="kanban-add-button", "Add"),
        ),
    )

@component
def KanbanLite():
    board, set_board = use_state(INITIAL_BOARD)

    def move_card(from_key, to_key, card):
        updated = {**board}
        updated[from_key] = [c for c in board[from_key] if c != card]
        updated[to_key] = [*board[to_key], card]
        set_board(updated)

    def add_card(col_key, title):
        updated = {**board}
        updated[col_key] = [*board[col_key], title]
        set_board(updated)

    return div(class_name="kanban-board",
        *[KanbanColumn(key=col["key"], col_key=col["key"], label=col["label"],
                        cards=board[col["key"]], on_move=move_card, on_add=add_card)
          for col in COLUMNS],
    )
```
