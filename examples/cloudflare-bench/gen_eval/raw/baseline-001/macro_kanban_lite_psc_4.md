<!-- exp=baseline-001 task=macro_kanban_lite cond=psc sample=4 model=claude-opus-4-8 cost_usd=0.063322 -->
```python
"use client"
R*

COLUMNS = ["Todo", "Doing", "Done"]

@c
def KanbanLite():
    cards, set_cards = us([
        ["Write spec", "Sketch UI"],
        ["Build API"],
        ["Setup repo"],
    ])
    drafts, set_drafts = us(["", "", ""])

    def move(col, idx, direction):
        next_cards = [[c for c in column] for column in cards]
        title = next_cards[col].pop(idx)
        next_cards[col + direction].append(title)
        set_cards(next_cards)

    def set_draft(col, value):
        next_drafts = [d for d in drafts]
        next_drafts[col] = value
        set_drafts(next_drafts)

    def add(col):
        title = drafts[col].strip()
        if title:
            next_cards = [[c for c in column] for column in cards]
            next_cards[col].append(title)
            set_cards(next_cards)
            set_draft(col, "")

    return div(cn="kanban",
        h1("KanbanLite"),
        div(cn="columns",
            *[div(key=col, cn="column",
                h2(cn="column-title", COLUMNS[col]),
                ul(cn="card-list",
                    *[li(key=f"{col}-{idx}", cn="card",
                        span(cn="card-title", title),
                        div(cn="card-actions",
                            (button(cn="move-left", oc=lambda col=col, idx=idx: move(col, idx, -1), "←")
                                if col > 0 else None),
                            (button(cn="move-right", oc=lambda col=col, idx=idx: move(col, idx, 1), "→")
                                if col < len(COLUMNS) - 1 else None),
                        ),
                    ) for idx, title in enumerate(column)]),
                div(cn="add-card",
                    input(value=drafts[col], ph="New card...",
                          oh=lambda e, col=col: set_draft(col, e.target.value)),
                    button(cn="add-btn", oc=lambda col=col: add(col), "Add"),
                ),
            ) for col, column in enumerate(cards)]),
    )
```
