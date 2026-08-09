<!-- exp=macrov2-opus task=macro_kanban_lite cond=ps sample=3 model=claude-opus-4-8 cost_usd=0.035393999999999995 -->
```python
"use client"

from pyths.react import component, use_state

COLUMNS = ["Todo", "Doing", "Done"]

INITIAL = {
    "Todo": ["Write spec", "Sketch UI"],
    "Doing": ["Build API"],
    "Done": ["Set up repo"],
}


@component
def KanbanLite():
    board, set_board = use_state(INITIAL)
    drafts, set_drafts = use_state({c: "" for c in COLUMNS})

    def move(col, idx, delta):
        target = COLUMNS[COLUMNS.index(col) + delta]
        next_board = {c: [t for t in board[c]] for c in COLUMNS}
        card = next_board[col].pop(idx)
        next_board[target].append(card)
        set_board(next_board)

    def add(col):
        title = drafts[col].strip()
        if not title:
            return
        next_board = {c: [t for t in board[c]] for c in COLUMNS}
        next_board[col].append(title)
        set_board(next_board)
        set_drafts({**drafts, col: ""})

    def set_draft(col, value):
        set_drafts({**drafts, col: value})

    return div(class_name="kanban",
        h1("KanbanLite"),
        div(class_name="kanban-board",
            *[column(key=col, name=col, cards=board[col],
                     draft=drafts[col], on_move=move,
                     on_add=add, on_draft=set_draft)
              for col in COLUMNS]),
    )


@component
def column(name, cards, draft, on_move, on_add, on_draft):
    is_first = name == COLUMNS[0]
    is_last = name == COLUMNS[-1]

    return div(class_name="kanban-column",
        h2(class_name="kanban-column-title", name),
        ul(class_name="kanban-cards",
            *[li(key=f"{name}-{i}", class_name="kanban-card",
                span(class_name="kanban-card-title", title),
                div(class_name="kanban-card-actions",
                    (button(class_name="kanban-move left",
                            on_click=lambda: on_move(name, i, -1),
                            "◀") if not is_first else None),
                    (button(class_name="kanban-move right",
                            on_click=lambda: on_move(name, i, 1),
                            "▶") if not is_last else None),
                ),
              )
              for i, title in enumerate(cards)]),
        div(class_name="kanban-add",
            input(class_name="kanban-input", value=draft,
                  placeholder=f"Add to {name}...",
                  on_change=lambda e: on_draft(name, e.target.value),
                  on_key_down=lambda e: on_add(name) if e.key == "Enter" else None),
            button(class_name="kanban-add-btn",
                   on_click=lambda: on_add(name), "Add"),
        ),
    )
```
