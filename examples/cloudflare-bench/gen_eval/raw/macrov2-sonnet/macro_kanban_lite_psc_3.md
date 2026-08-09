<!-- exp=macrov2-sonnet task=macro_kanban_lite cond=psc sample=3 model=claude-sonnet-5 cost_usd=0.0766932 -->
```python
"use client"
R*

COLUMNS = ["Todo", "Doing", "Done"]
KEYS = ["todo", "doing", "done"]

INITIAL_CARDS = {
    "todo": ["Design schema", "Write tests"],
    "doing": ["Build API"],
    "done": ["Setup repo"],
}

@c
def KanbanLite():
    cards, set_cards = us(INITIAL_CARDS)
    inputs, set_inputs = us({"todo": "", "doing": "", "done": ""})

    def move(col_idx, index, direction):
        source_key = KEYS[col_idx]
        target_key = KEYS[col_idx + direction]
        card = cards[source_key][index]
        new_source = [c for j, c in enumerate(cards[source_key]) if j != index]
        new_target = [*cards[target_key], card]
        set_cards({**cards, source_key: new_source, target_key: new_target})

    def add(col_idx):
        key = KEYS[col_idx]
        text = inputs[key].strip()
        if text:
            set_cards({**cards, key: [*cards[key], text]})
            set_inputs({**inputs, key: ""})

    def set_input(col_idx, value):
        key = KEYS[col_idx]
        set_inputs({**inputs, key: value})

    return div(cn="kanban-board",
        *[div(key=label, cn="kanban-column",
            h2(cn="kanban-column-title", label),
            ul(cn="kanban-card-list",
                *[li(key=idx, cn="kanban-card",
                    span(cn="kanban-card-title", title),
                    div(cn="kanban-card-actions",
                        button(oc=lambda: move(i, idx, -1), cn="kanban-move-btn", "<") if i > 0 else None,
                        button(oc=lambda: move(i, idx, 1), cn="kanban-move-btn", ">") if i < len(KEYS) - 1 else None,
                    ),
                ) for idx, title in enumerate(cards[KEYS[i]])],
            ),
            div(cn="kanban-add-row",
                input(value=inputs[KEYS[i]], oh=lambda e: set_input(i, e.target.value),
                      ph="New card...", cn="kanban-add-input"),
                button(oc=lambda: add(i), cn="kanban-add-btn", "Add"),
            ),
        ) for i, label in enumerate(COLUMNS)],
    )
```
