<!-- exp=macrov2-sonnet task=macro_kanban_lite cond=ps sample=3 model=claude-sonnet-5 cost_usd=0.0452616 -->
```python
"use client"

from pyths.react import component, use_state

COLUMNS = ["Todo", "Doing", "Done"]

INITIAL_CARDS = {
    "Todo": ["Design mockups", "Write spec"],
    "Doing": ["Build API"],
    "Done": ["Set up repo"],
}

@component
def KanbanLite():
    cards, set_cards = use_state(INITIAL_CARDS)
    inputs, set_inputs = use_state({"Todo": "", "Doing": "", "Done": ""})

    def move_card(col, idx, direction):
        col_index = COLUMNS.index(col)
        target_index = col_index + direction
        if target_index < 0 or target_index >= len(COLUMNS):
            return
        target_col = COLUMNS[target_index]
        title = cards[col][idx]
        new_source = [t for i, t in enumerate(cards[col]) if i != idx]
        new_target = [*cards[target_col], title]
        set_cards({**cards, col: new_source, target_col: new_target})

    def add_card(col):
        text = inputs[col].strip()
        if not text:
            return
        set_cards({**cards, col: [*cards[col], text]})
        set_inputs({**inputs, col: ""})

    def update_input(col, value):
        set_inputs({**inputs, col: value})

    return div(class_name="kanban-board",
        *[div(key=col, class_name="kanban-column",
            h2(class_name="kanban-column-title", col),
            ul(class_name="kanban-card-list",
                *[li(key=i, class_name="kanban-card",
                    span(class_name="kanban-card-title", title),
                    div(class_name="kanban-card-actions",
                        button(class_name="kanban-move-btn", on_click=lambda: move_card(col, i, -1), "←")
                        if COLUMNS.index(col) > 0 else None,
                        button(class_name="kanban-move-btn", on_click=lambda: move_card(col, i, 1), "→")
                        if COLUMNS.index(col) < len(COLUMNS) - 1 else None,
                    ),
                ) for i, title in enumerate(cards[col])]
            ),
            div(class_name="kanban-add-form",
                input(class_name="kanban-input", value=inputs[col], placeholder="New card...",
                      on_change=lambda e: update_input(col, e.target.value)),
                button(class_name="kanban-add-btn", on_click=lambda: add_card(col), "Add"),
            ),
        ) for col in COLUMNS]
    )
```
