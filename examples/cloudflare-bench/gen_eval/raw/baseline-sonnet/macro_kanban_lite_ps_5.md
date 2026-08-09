<!-- exp=baseline-sonnet task=macro_kanban_lite cond=ps sample=5 model=claude-sonnet-5 cost_usd=0.0242769 -->
```python
"use client"

from pyths.react import component, use_state

COLUMNS = ["Todo", "Doing", "Done"]

INITIAL_CARDS = {
    "Todo": ["Write proposal", "Research vendors"],
    "Doing": ["Design mockups"],
    "Done": ["Kickoff meeting"],
}

def move_card(cards, col, index, direction):
    order = COLUMNS
    col_index = order.index(col)
    target_index = col_index + direction
    if target_index < 0 or target_index >= len(order):
        return cards
    target_col = order[target_index]
    card = cards[col][index]
    new_source = [c for i, c in enumerate(cards[col]) if i != index]
    new_target = [*cards[target_col], card]
    return {**cards, col: new_source, target_col: new_target}

@component
def KanbanLite():
    cards, set_cards = use_state(INITIAL_CARDS)
    drafts, set_drafts = use_state({"Todo": "", "Doing": "", "Done": ""})

    def add_card(col):
        text = drafts[col].strip()
        if text:
            set_cards({**cards, col: [*cards[col], text]})
            set_drafts({**drafts, col: ""})

    def set_draft(col, value):
        set_drafts({**drafts, col: value})

    def move(col, index, direction):
        set_cards(move_card(cards, col, index, direction))

    return div(class_name="kanban-board",
        *[div(key=col, class_name="kanban-column",
            h2(class_name="kanban-column-title", col),
            ul(class_name="kanban-card-list",
                *[li(key=i, class_name="kanban-card",
                    span(class_name="kanban-card-title", title),
                    div(class_name="kanban-card-actions",
                        button(class_name="kanban-move-btn", on_click=lambda i=i, col=col: move(col, i, -1), "<")
                        if COLUMNS.index(col) > 0 else None,
                        button(class_name="kanban-move-btn", on_click=lambda i=i, col=col: move(col, i, 1), ">")
                        if COLUMNS.index(col) < len(COLUMNS) - 1 else None,
                    ),
                ) for i, title in enumerate(cards[col])],
            ),
            div(class_name="kanban-add-row",
                input(class_name="kanban-add-input", value=drafts[col],
                    placeholder="New card...",
                    on_change=lambda e, col=col: set_draft(col, e.target.value)),
                button(class_name="kanban-add-btn", on_click=lambda col=col: add_card(col), "Add"),
            ),
        ) for col in COLUMNS],
    )
```
