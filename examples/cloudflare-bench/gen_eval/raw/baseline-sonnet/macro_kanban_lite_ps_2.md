<!-- exp=baseline-sonnet task=macro_kanban_lite cond=ps sample=2 model=claude-sonnet-5 cost_usd=0.052791899999999996 -->
```python
"use client"

from pyths.react import component, use_state

COLUMNS = ["Todo", "Doing", "Done"]

INITIAL_CARDS = {
    "Todo": ["Design landing page", "Write tests"],
    "Doing": ["Implement auth"],
    "Done": ["Set up repo"],
}

INITIAL_INPUTS = {"Todo": "", "Doing": "", "Done": ""}


def move_card(cards, column, index, direction):
    col_idx = COLUMNS.index(column)
    new_col_idx = col_idx + direction
    if new_col_idx < 0 or new_col_idx >= len(COLUMNS):
        return cards
    from_col = COLUMNS[col_idx]
    to_col = COLUMNS[new_col_idx]
    card = cards[from_col][index]
    new_from = [c for i, c in enumerate(cards[from_col]) if i != index]
    new_to = [*cards[to_col], card]
    return {**cards, from_col: new_from, to_col: new_to}


@component
def KanbanLite():
    cards, set_cards = use_state(INITIAL_CARDS)
    inputs, set_inputs = use_state(INITIAL_INPUTS)

    def handle_move(column, index, direction):
        set_cards(move_card(cards, column, index, direction))

    def handle_input_change(column, value):
        set_inputs({**inputs, column: value})

    def handle_add(column):
        text = inputs[column].strip()
        if text:
            set_cards({**cards, column: [*cards[column], text]})
            set_inputs({**inputs, column: ""})

    def render_card(column, index, title):
        col_idx = COLUMNS.index(column)
        can_left = col_idx > 0
        can_right = col_idx < len(COLUMNS) - 1
        return div(class_name="kanban-card", key=index,
            span(class_name="kanban-card-title", title),
            div(class_name="kanban-card-actions",
                button(class_name="kanban-move-btn", on_click=lambda: handle_move(column, index, -1), "<") if can_left else None,
                button(class_name="kanban-move-btn", on_click=lambda: handle_move(column, index, 1), ">") if can_right else None,
            ),
        )

    def render_column(column):
        return div(class_name="kanban-column", key=column,
            h3(class_name="kanban-column-title", column),
            div(class_name="kanban-card-list",
                *[render_card(column, i, title) for i, title in enumerate(cards[column])],
            ),
            div(class_name="kanban-add-row",
                input(class_name="kanban-add-input", value=inputs[column],
                      placeholder=f"New {column} card...",
                      on_change=lambda e: handle_input_change(column, e.target.value)),
                button(class_name="kanban-add-btn", on_click=lambda: handle_add(column), "Add"),
            ),
        )

    return div(class_name="kanban-board",
        *[render_column(column) for column in COLUMNS],
    )
```
