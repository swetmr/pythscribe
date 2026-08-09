<!-- exp=baseline-haiku task=macro_kanban_lite cond=ps sample=1 model=claude-haiku-4-5-20251001 cost_usd=0.0563743 -->
```python
from pyths.react import component, use_state

@component
def KanbanLite():
    initial_cards = {
        "todo": ["Design homepage", "Setup database"],
        "doing": ["Implement auth"],
        "done": ["Project setup"]
    }
    
    cards, set_cards = use_state(initial_cards)
    inputs, set_inputs = use_state({"todo": "", "doing": "", "done": ""})
    
    column_order = ["todo", "doing", "done"]
    
    def move_card(col, card_idx, direction):
        col_idx = column_order.index(col)
        next_col = None
        
        if direction == "left" and col_idx > 0:
            next_col = column_order[col_idx - 1]
        elif direction == "right" and col_idx < len(column_order) - 1:
            next_col = column_order[col_idx + 1]
        
        if next_col:
            card = cards[col][card_idx]
            new_cards = {**cards}
            new_cards[col] = [c for i, c in enumerate(cards[col]) if i != card_idx]
            new_cards[next_col] = [*new_cards[next_col], card]
            set_cards(new_cards)
    
    def add_card(col, title):
        if title.strip():
            new_cards = {**cards}
            new_cards[col] = [*cards[col], title]
            set_cards(new_cards)
            new_inputs = {**inputs}
            new_inputs[col] = ""
            set_inputs(new_inputs)
    
    def render_column(col_name, col_title):
        col_idx = column_order.index(col_name)
        
        def on_change(e):
            new_inputs = {**inputs}
            new_inputs[col_name] = e.target.value
            set_inputs(new_inputs)
        
        def on_add():
            add_card(col_name, inputs[col_name])
        
        return div(class_name="kanban-column",
            h3(col_title),
            div(class_name="kanban-cards",
                *[div(class_name="kanban-card",
                    span(card),
                    (button(on_click=lambda i=i: move_card(col_name, i, "left"),
                           class_name="kanban-btn-move-left", "←") if col_idx > 0 else None),
                    (button(on_click=lambda i=i: move_card(col_name, i, "right"),
                           class_name="kanban-btn-move-right", "→") if col_idx < len(column_order) - 1 else None),
                ) for i, card in enumerate(cards[col_name])]
            ),
            input(value=inputs[col_name], 
                  on_change=on_change,
                  placeholder=f"Add to {col_title}...", class_name="kanban-input-add"),
            button(on_click=on_add, class_name="kanban-btn-add", "Add"),
        )
    
    return div(class_name="kanban-lite",
        div(class_name="kanban-columns",
            render_column("todo", "Todo"),
            render_column("doing", "Doing"),
            render_column("done", "Done"),
        ),
    )
```
