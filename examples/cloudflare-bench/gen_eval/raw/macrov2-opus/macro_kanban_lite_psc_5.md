<!-- exp=macrov2-opus task=macro_kanban_lite cond=psc sample=5 model=claude-opus-4-8 cost_usd=0.037954999999999996 -->
```python
"use client"
R*

COLUMNS = ["todo", "doing", "done"]
LABELS = {"todo": "Todo", "doing": "Doing", "done": "Done"}
INITIAL = {
    "todo": ["Write spec", "Sketch UI"],
    "doing": ["Build API"],
    "done": ["Set up repo"],
}

@c
def KanbanLite():
    cards, set_cards = us(INITIAL)
    drafts, set_drafts = us({"todo": "", "doing": "", "done": ""})

    def move(col, i, delta):
        ci = COLUMNS.index(col)
        target = COLUMNS[ci + delta]
        next_cards = {k: [c for c in v] for k, v in cards.items()}
        title = next_cards[col].pop(i)
        next_cards[target].append(title)
        set_cards(next_cards)

    def add(col):
        title = drafts[col].strip()
        if title:
            set_cards({**cards, col: [*cards[col], title]})
            set_drafts({**drafts, col: ""})

    def make_column(col):
        ci = COLUMNS.index(col)
        return div(cn="kanban-column", key=col,
            h2(cn="kanban-heading", LABELS[col]),
            ul(cn="kanban-list",
                *[li(cn="kanban-card", key=f"{col}-{i}",
                    span(cn="kanban-title", title),
                    div(cn="kanban-actions",
                        (button(cn="kanban-move", oc=lambda: move(col, i, -1), "←")
                            if ci > 0 else None),
                        (button(cn="kanban-move", oc=lambda: move(col, i, 1), "→")
                            if ci < len(COLUMNS) - 1 else None),
                    ),
                ) for i, title in enumerate(cards[col])]),
            div(cn="kanban-add",
                input(cn="kanban-input", value=drafts[col],
                    ph="New card...",
                    oh=lambda e: set_drafts({**drafts, col: e.target.value})),
                button(cn="kanban-add-btn", oc=lambda: add(col), "Add"),
            ),
        )

    return div(cn="kanban",
        *[make_column(col) for col in COLUMNS])
```
