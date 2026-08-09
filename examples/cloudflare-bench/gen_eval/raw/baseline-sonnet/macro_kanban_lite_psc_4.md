<!-- exp=baseline-sonnet task=macro_kanban_lite cond=psc sample=4 model=claude-sonnet-5 cost_usd=0.060285899999999996 -->
```python
"use client"
R*

COLUMNS = ["Todo", "Doing", "Done"]
INITIAL_CARDS = {
    "Todo": ["Design mockups", "Write specs"],
    "Doing": ["Build API"],
    "Done": ["Setup repo"],
}

@c
def KanbanLite():
    cards, set_cards = us(INITIAL_CARDS)
    inputs, set_inputs = us({"Todo": "", "Doing": "", "Done": ""})

    def move_card(col, idx, direction):
        target = COLUMNS[COLUMNS.index(col) + direction]
        remaining = [c for c in cards[col]]
        card = remaining.pop(idx)
        set_cards({**cards, col: remaining, target: [*cards[target], card]})

    def add_card(col):
        text = inputs[col].strip()
        if text:
            set_cards({**cards, col: [*cards[col], text]})
            set_inputs({**inputs, col: ""})

    def render_column(col):
        ci = COLUMNS.index(col)
        can_left = ci > 0
        can_right = ci < len(COLUMNS) - 1
        return div(cn="kanban-column", key=col,
            h3(cn="kanban-header", col),
            div(cn="kanban-cards",
                *[div(cn="kanban-card", key=idx,
                    button(cn="kanban-move", oc=lambda c=col, i=idx: move_card(c, i, -1), "<") if can_left else None,
                    span(cn="kanban-title", title),
                    button(cn="kanban-move", oc=lambda c=col, i=idx: move_card(c, i, 1), ">") if can_right else None,
                  ) for idx, title in enumerate(cards[col])],
            ),
            div(cn="kanban-add",
                input(cn="kanban-input", value=inputs[col], ph="New card...",
                    oh=lambda e, c=col: set_inputs({**inputs, c: e.target.value})),
                button(cn="kanban-add-btn", oc=lambda c=col: add_card(c), "Add"),
            ),
        )

    return div(cn="kanban-board",
        *[render_column(col) for col in COLUMNS],
    )
```
