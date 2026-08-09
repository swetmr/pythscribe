<!-- exp=macrov2-haiku task=macro_kanban_lite cond=ps sample=2 model=claude-haiku-4-5-20251001 cost_usd=0.0469447 -->
```python
from pyths.react import component, use_state

@component
def KanbanLite():
    columns, set_columns = use_state([
        {"name": "Todo", "cards": ["Design homepage", "Setup database"]},
        {"name": "Doing", "cards": ["Implement auth"]},
        {"name": "Done", "cards": ["Project setup"]},
    ])
    
    inputs, set_inputs = use_state(["", "", ""])
    
    def add_card(col_idx):
        if inputs[col_idx].strip():
            new_columns = [c for c in columns]
            new_columns[col_idx] = {
                **new_columns[col_idx],
                "cards": [*new_columns[col_idx]["cards"], inputs[col_idx]]
            }
            set_columns(new_columns)
            new_inputs = [i for i in inputs]
            new_inputs[col_idx] = ""
            set_inputs(new_inputs)
    
    def move_card(from_col, card_idx, to_col):
        if to_col < 0 or to_col >= len(columns):
            return
        
        new_columns = [c for c in columns]
        card = new_columns[from_col]["cards"][card_idx]
        
        new_columns[from_col] = {
            **new_columns[from_col],
            "cards": new_columns[from_col]["cards"][:card_idx] + new_columns[from_col]["cards"][card_idx+1:]
        }
        
        new_columns[to_col] = {
            **new_columns[to_col],
            "cards": [*new_columns[to_col]["cards"], card]
        }
        
        set_columns(new_columns)
    
    def on_input_change(col_idx, value):
        new_inputs = [i for i in inputs]
        new_inputs[col_idx] = value
        set_inputs(new_inputs)
    
    return div(class_name="kanban-lite",
        div(class_name="kanban-columns",
            *[div(class_name="kanban-column",
                h3(col["name"]),
                div(class_name="card-list",
                    *[div(class_name="card", key=i,
                        div(class_name="card-content", card_text),
                        div(class_name="card-buttons",
                            button(
                                class_name="move-btn",
                                on_click=lambda: move_card(col_idx, i, col_idx - 1),
                                "◀"
                            ) if col_idx > 0 else None,
                            button(
                                class_name="move-btn",
                                on_click=lambda: move_card(col_idx, i, col_idx + 1),
                                "▶"
                            ) if col_idx < len(columns) - 1 else None,
                        )
                    ) for i, card_text in enumerate(col["cards"])]
                ),
                div(class_name="add-card",
                    input(
                        type="text",
                        value=inputs[col_idx],
                        on_change=lambda e: on_input_change(col_idx, e.target.value),
                        placeholder="New card..."
                    ),
                    button(on_click=lambda: add_card(col_idx), "Add")
                )
            ) for col_idx, col in enumerate(columns)]
        )
    )
```
