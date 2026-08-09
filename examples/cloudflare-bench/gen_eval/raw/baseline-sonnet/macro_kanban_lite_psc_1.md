<!-- exp=baseline-sonnet task=macro_kanban_lite cond=psc sample=1 model=claude-sonnet-5 cost_usd=0.083196 -->
```python
"use client"
R*

@c
def KanbanLite():
    cards, set_cards = us({
        "Todo": ["Design mockups", "Write specs"],
        "Doing": ["Build API"],
        "Done": ["Setup repo"],
    })
    inputs, set_inputs = us({"Todo": "", "Doing": "", "Done": ""})
    columns = ["Todo", "Doing", "Done"]

    def move(col, idx, delta):
        i = columns.index(col)
        target = columns[i + delta]
        updated = {**cards}
        updated[col] = [c for j, c in enumerate(cards[col]) if j != idx]
        updated[target] = [*cards[target], cards[col][idx]]
        set_cards(updated)

    def add_card(col):
        text = inputs[col]
        if text:
            updated = {**cards}
            updated[col] = [*cards[col], text]
            set_cards(updated)
            set_inputs({**inputs, col: ""})

    def render_card(col, idx, title):
        i = columns.index(col)
        can_left = i > 0
        can_right = i < len(columns) - 1
        return li(cn="kanban-card", key=idx,
            span(cn="kanban-card-title", title),
            div(cn="kanban-card-actions",
                button(oc=lambda: move(col, idx, -1), cn="kanban-move-left", "←") if can_left else None,
                button(oc=lambda: move(col, idx, 1), cn="kanban-move-right", "→") if can_right else None,
            ),
        )

    def render_column(col):
        return div(cn="kanban-column", key=col,
            h3(cn="kanban-column-title", col),
            ul(cn="kanban-card-list",
                *[render_card(col, idx, title) for idx, title in enumerate(cards[col])],
            ),
            div(cn="kanban-add",
                input(value=inputs[col],
                      oh=lambda e: set_inputs({**inputs, col: e.target.value}),
                      ph="Add a card...", cn="kanban-input"),
                button(oc=lambda: add_card(col), cn="kanban-add-button", "Add"),
            ),
        )

    return div(cn="kanban-board",
        *[render_column(col) for col in columns],
    )
```
