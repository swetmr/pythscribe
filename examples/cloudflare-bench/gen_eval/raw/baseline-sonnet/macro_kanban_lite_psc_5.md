<!-- exp=baseline-sonnet task=macro_kanban_lite cond=psc sample=5 model=claude-sonnet-5 cost_usd=0.0239409 -->
```python
"use client"
R*

COLUMNS = ["todo", "doing", "done"]
LABELS = {"todo": "Todo", "doing": "Doing", "done": "Done"}

def initial_cards():
    return {
        "todo": ["Write spec", "Plan sprint"],
        "doing": ["Build API"],
        "done": ["Set up repo"],
    }

@c
def KanbanLite():
    cards, set_cards = us(initial_cards())
    drafts, set_drafts = us({"todo": "", "doing": "", "done": ""})

    def move(col, idx, direction):
        i = COLUMNS.index(col)
        target = COLUMNS[i + direction]
        title = cards[col][idx]
        new_cards = {k: [c for c in v] for k, v in cards.items()}
        new_cards[col].pop(idx)
        new_cards[target].append(title)
        set_cards(new_cards)

    def add_card(col):
        title = drafts[col].strip()
        if not title:
            return
        new_cards = {k: [c for c in v] for k, v in cards.items()}
        new_cards[col].append(title)
        set_cards(new_cards)
        set_drafts({**drafts, col: ""})

    def set_draft(col, value):
        set_drafts({**drafts, col: value})

    return div(cn="kanban-board",
        *[div(cn="kanban-column", key=col,
            h3(LABELS[col]),
            ul(cn="kanban-cards",
                *[li(cn="kanban-card", key=f"{col}-{idx}",
                    button(oc=lambda col=col, idx=idx: move(col, idx, -1), dis=(COLUMNS.index(col) == 0), "<") if COLUMNS.index(col) > 0 else None,
                    span(cn="kanban-title", title),
                    button(oc=lambda col=col, idx=idx: move(col, idx, 1), dis=(COLUMNS.index(col) == len(COLUMNS) - 1), ">") if COLUMNS.index(col) < len(COLUMNS) - 1 else None,
                ) for idx, title in enumerate(cards[col])]
            ),
            div(cn="kanban-add",
                input(value=drafts[col], oh=lambda e, col=col: set_draft(col, e.target.value), ph="New card..."),
                button(oc=lambda col=col: add_card(col), "Add"),
            ),
        ) for col in COLUMNS]
    )
```
