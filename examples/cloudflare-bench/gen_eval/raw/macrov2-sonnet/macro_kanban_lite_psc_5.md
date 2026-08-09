<!-- exp=macrov2-sonnet task=macro_kanban_lite cond=psc sample=5 model=claude-sonnet-5 cost_usd=0.055588200000000004 -->
```python
"use client"
R*

COLUMNS = [("todo", "Todo"), ("doing", "Doing"), ("done", "Done")]

@c
def KanbanLite():
    cards, set_cards = us({
        "todo": ["Design mockups", "Research competitors"],
        "doing": ["Build API"],
        "done": ["Setup repo"],
    })
    inputs, set_inputs = us({"todo": "", "doing": "", "done": ""})

    def move(col, idx, delta):
        keys = [k for k, _ in COLUMNS]
        ci = keys.index(col)
        ni = ci + delta
        if ni < 0 or ni >= len(keys):
            return
        target = keys[ni]
        title = cards[col][idx]
        new_cards = {**cards}
        new_cards[col] = [c for i, c in enumerate(cards[col]) if i != idx]
        new_cards[target] = [*cards[target], title]
        set_cards(new_cards)

    def add_card(col):
        text = inputs[col].strip()
        if not text:
            return
        set_cards({**cards, col: [*cards[col], text]})
        set_inputs({**inputs, col: ""})

    return div(cn="kanban-board",
        *[div(key=col_key, cn="kanban-column",
            h3(label),
            ul(cn="kanban-cards",
                *[li(key=i, cn="kanban-card",
                    span(cn="kanban-card-title", title),
                    div(cn="kanban-card-actions",
                        button(oc=lambda: move(col_key, i, -1), "<") if ci > 0 else None,
                        button(oc=lambda: move(col_key, i, 1), ">") if ci < len(COLUMNS) - 1 else None,
                    ),
                ) for i, title in enumerate(cards[col_key])],
            ),
            div(cn="kanban-add",
                input(value=inputs[col_key],
                      oh=lambda e: set_inputs({**inputs, col_key: e.target.value}),
                      ph="New card..."),
                button(oc=lambda: add_card(col_key), "Add"),
            ),
        ) for ci, (col_key, label) in enumerate(COLUMNS)],
    )
```
