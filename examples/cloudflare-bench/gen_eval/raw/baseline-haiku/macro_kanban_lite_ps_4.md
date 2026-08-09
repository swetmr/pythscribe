<!-- exp=baseline-haiku task=macro_kanban_lite cond=ps sample=4 model=claude-haiku-4-5-20251001 cost_usd=0.0582925 -->
```python
"use client"

from pyths.react import component, use_state

@component
def KanbanLite():
    cards, set_cards = use_state({
        "todo": [
            {"id": 0, "text": "Task 1"},
            {"id": 1, "text": "Task 2"}
        ],
        "doing": [
            {"id": 2, "text": "Task 3"}
        ],
        "done": []
    })
    
    next_id, set_next_id = use_state(3)
    
    inputs, set_inputs = use_state({
        "todo": "",
        "doing": "",
        "done": ""
    })
    
    columns = ["todo", "doing", "done"]
    
    def add_card(column):
        text = inputs[column].strip()
        if text:
            updated = {**cards}
            updated[column] = [*cards[column], {"id": next_id, "text": text}]
            set_cards(updated)
            set_next_id(next_id + 1)
            updated_inputs = {**inputs}
            updated_inputs[column] = ""
            set_inputs(updated_inputs)
    
    def move_card(from_col, index, to_col):
        updated = {**cards}
        card = updated[from_col][index]
        updated[from_col] = [c for i, c in enumerate(updated[from_col]) if i != index]
        updated[to_col] = [*updated[to_col], card]
        set_cards(updated)
    
    def move_left(column, index):
        col_idx = columns.index(column)
        if col_idx > 0:
            move_card(column, index, columns[col_idx - 1])
    
    def move_right(column, index):
        col_idx = columns.index(column)
        if col_idx < 2:
            move_card(column, index, columns[col_idx + 1])
    
    def render_column(col):
        col_idx = columns.index(col)
        
        return div(class_name="kanban-column",
            h3(col.capitalize()),
            input(
                value=inputs[col],
                on_change=lambda e, c=col: set_inputs({**inputs, c: e.target.value}),
                placeholder=f"New {col}..."
            ),
            button(
                on_click=lambda c=col: add_card(c),
                "Add"
            ),
            div(class_name="card-list",
                *[div(class_name="card",
                    key=card["id"],
                    div(class_name="card-buttons",
                        button(
                            on_click=lambda c=col, i=idx: move_left(c, i),
                            disabled=(col_idx == 0),
                            "←"
                        ),
                        button(
                            on_click=lambda c=col, i=idx: move_right(c, i),
                            disabled=(col_idx == 2),
                            "→"
                        )
                    ),
                    p(card["text"])
                )
                for idx, card in enumerate(cards[col])]
            )
        )
    
    return div(class_name="kanban-lite",
        *[render_column(col) for col in columns]
    )
```
