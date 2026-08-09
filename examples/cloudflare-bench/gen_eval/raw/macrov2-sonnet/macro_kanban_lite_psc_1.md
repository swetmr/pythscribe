<!-- exp=macrov2-sonnet task=macro_kanban_lite cond=psc sample=1 model=claude-sonnet-5 cost_usd=0.051639599999999994 -->
```python
"use client"
R*

@c
def KanbanLite():
    columns, set_columns = us({
        "Todo": ["Design landing page", "Write tests"],
        "Doing": ["Fix login bug"],
        "Doing": ["Fix login bug"],
        "Done": ["Setup repo"],
    })
    drafts, set_drafts = us({"Todo": "", "Doing": "", "Done": ""})

    order = ["Todo", "Doing", "Done"]

    def move(col, idx, direction):
        col_idx = order.index(col)
        target_idx = col_idx + direction
        if target_idx < 0 or target_idx >= len(order):
            return
        target_col = order[target_idx]
        card = columns[col][idx]
        new_source = [c for i, c in enumerate(columns[col]) if i != idx]
        new_target = [*columns[target_col], card]
        set_columns({**columns, col: new_source, target_col: new_target})

    def add_card(col):
        text = drafts[col].strip()
        if not text:
            return
        set_columns({**columns, col: [*columns[col], text]})
        set_drafts({**drafts, col: ""})

    def mk_card(col, idx, title):
        can_left = order.index(col) > 0
        can_right = order.index(col) < len(order) - 1
        return div(key=idx, cn="kanban-card",
            span(cn="kanban-card-title", title),
            div(cn="kanban-card-actions",
                button(oc=lambda: move(col, idx, -1), dis=not can_left, "<") if can_left else None,
                button(oc=lambda: move(col, idx, 1), dis=not can_right, ">") if can_right else None,
            ),
        )

    def mk_column(col):
        return div(key=col, cn="kanban-column",
            h3(cn="kanban-column-title", col),
            div(cn="kanban-card-list",
                *[mk_card(col, i, title) for i, title in enumerate(columns[col])],
            ),
            div(cn="kanban-add",
                input(value=drafts[col], ph="New card...",
                      oh=lambda e: set_drafts({**drafts, col: e.target.value})),
                button(oc=lambda: add_card(col), "Add"),
            ),
        )

    return div(cn="kanban-board",
        *[mk_column(col) for col in order],
    )
```
