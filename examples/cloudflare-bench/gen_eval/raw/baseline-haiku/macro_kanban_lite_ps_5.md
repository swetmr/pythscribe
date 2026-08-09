<!-- exp=baseline-haiku task=macro_kanban_lite cond=ps sample=5 model=claude-haiku-4-5-20251001 cost_usd=0.0397225 -->
```python
"use client"

from pyths.react import component, use_state

@component
def KanbanLite():
    columns, set_columns = use_state({
        "todo": [
            {"id": 1, "title": "Design wireframes"},
            {"id": 2, "title": "Plan architecture"},
        ],
        "doing": [
            {"id": 3, "title": "Implement backend"},
        ],
        "done": [
            {"id": 4, "title": "Setup project"},
        ],
    })
    
    inputs, set_inputs = use_state({
        "todo": "",
        "doing": "",
        "done": "",
    })
    
    next_id, set_next_id = use_state(5)
    
    col_order = ["todo", "doing", "done"]
    col_names = {"todo": "Todo", "doing": "Doing", "done": "Done"}
    
    def move_card(col, card_id, direction):
        col_idx = col_order.index(col)
        new_col_idx = col_idx + direction
        
        if 0 <= new_col_idx < len(col_order):
            new_col = col_order[new_col_idx]
            new_columns = {k: list(v) for k, v in columns.items()}
            
            card_to_move = None
            for i, card in enumerate(new_columns[col]):
                if card["id"] == card_id:
                    card_to_move = new_columns[col].pop(i)
                    break
            
            if card_to_move:
                new_columns[new_col].append(card_to_move)
                set_columns(new_columns)
    
    def add_card(col):
        text = inputs[col].strip()
        if text:
            new_columns = {k: list(v) for k, v in columns.items()}
            new_columns[col].append({"id": next_id, "title": text})
            set_columns(new_columns)
            
            new_inputs = dict(inputs)
            new_inputs[col] = ""
            set_inputs(new_inputs)
            
            set_next_id(next_id + 1)
    
    def update_input(col, value):
        new_inputs = dict(inputs)
        new_inputs[col] = value
        set_inputs(new_inputs)
    
    def render_column(col):
        col_idx = col_order.index(col)
        
        cards = [
            div(class_name="card", key=card["id"],
                div(class_name="card-title", card["title"]),
                div(class_name="card-buttons",
                    button(class_name="move-btn", on_click=lambda cid=card["id"], c=col: move_card(c, cid, -1), "←") if col_idx > 0 else None,
                    button(class_name="move-btn", on_click=lambda cid=card["id"], c=col: move_card(c, cid, 1), "→") if col_idx < len(col_order) - 1 else None,
                ),
            )
            for card in columns[col]
        ]
        
        return div(class_name="kanban-column",
            h2(col_names[col]),
            div(class_name="input-group",
                input(type="text", value=inputs[col], on_change=lambda e, c=col: update_input(c, e.target.value), placeholder=f"Add to {col_names[col]}...", class_name="card-input"),
                button(on_click=lambda c=col: add_card(c), class_name="add-btn", "Add"),
            ),
            div(class_name="cards-list", *cards),
        )
    
    return div(class_name="kanban-container",
        *[render_column(col) for col in col_order],
    )
```
