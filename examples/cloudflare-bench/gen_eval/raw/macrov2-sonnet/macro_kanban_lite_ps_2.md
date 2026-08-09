<!-- exp=macrov2-sonnet task=macro_kanban_lite cond=ps sample=2 model=claude-sonnet-5 cost_usd=0.04791659999999999 -->
```python
"use client"

from pyths.react import component, use_state

COLUMNS = ["Todo", "Doing", "Done"]

@component
def CardView(col, idx, title, col_index):
    move = col.move
    return li(class_name="kanban-card", key=idx,
        button(class_name="kanban-move-btn kanban-move-left",
               on_click=lambda: col.move(col.name, idx, -1),
               "◀") if col_index > 0 else None,
        span(class_name="kanban-card-title", title),
        button(class_name="kanban-move-btn kanban-move-right",
               on_click=lambda: col.move(col.name, idx, 1),
               "▶") if col_index < len(COLUMNS) - 1 else None,
    )

@component
def ColumnView(name, items, input_value, on_move, on_add, on_input_change):
    col_index = COLUMNS.index(name)
    return div(class_name="kanban-column", key=name,
        h3(class_name="kanban-column-title", name),
        ul(class_name="kanban-card-list",
            *[li(class_name="kanban-card", key=idx,
                (button(class_name="kanban-move-btn kanban-move-left",
                        on_click=lambda: on_move(name, idx, -1),
                        "◀") if col_index > 0 else None),
                span(class_name="kanban-card-title", title),
                (button(class_name="kanban-move-btn kanban-move-right",
                        on_click=lambda: on_move(name, idx, 1),
                        "▶") if col_index < len(COLUMNS) - 1 else None),
              )
              for idx, title in enumerate(items)]
        ),
        div(class_name="kanban-add-row",
            input(class_name="kanban-input", value=input_value,
                  placeholder=f"Add to {name}...",
                  on_change=lambda e: on_input_change(name, e.target.value)),
            button(class_name="kanban-add-btn", on_click=lambda: on_add(name), "Add"),
        ),
    )

@component
def KanbanLite():
    cards, set_cards = use_state({
        "Todo": ["Write spec", "Design schema"],
        "Doing": ["Build API"],
        "Done": ["Setup repo"],
    })
    inputs, set_inputs = use_state({"Todo": "", "Doing": "", "Done": ""})

    def move(col, idx, direction):
        col_index = COLUMNS.index(col)
        target_index = col_index + direction
        if target_index < 0 or target_index >= len(COLUMNS):
            return
        target_col = COLUMNS[target_index]
        title = cards[col][idx]
        new_source = [t for i, t in enumerate(cards[col]) if i != idx]
        new_target = [*cards[target_col], title]
        set_cards({**cards, col: new_source, target_col: new_target})

    def add_card(col):
        text = inputs[col].strip()
        if not text:
            return
        set_cards({**cards, col: [*cards[col], text]})
        set_inputs({**inputs, col: ""})

    def update_input(col, value):
        set_inputs({**inputs, col: value})

    return div(class_name="kanban-board",
        *[ColumnView(col, cards[col], inputs[col], move, add_card, update_input)
          for col in COLUMNS],
    )
```
