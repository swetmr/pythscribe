<!-- exp=baseline-haiku task=macro_kanban_lite cond=psc sample=3 model=claude-haiku-4-5-20251001 cost_usd=0.042536700000000004 -->
```python
"use client"
R*

@c
def KanbanLite():
    cards, set_cards = us({
        "todo": ["Design mockups", "Write documentation"],
        "doing": ["Build component"],
        "done": ["Setup repository"],
    })
    
    inputs, set_inputs = us({"todo": "", "doing": "", "done": ""})
    
    def move_card(col, idx, direction):
        cols = ["todo", "doing", "done"]
        old_idx = cols.index(col)
        new_idx = old_idx + direction
        if 0 <= new_idx < len(cols):
            target = cols[new_idx]
            card = cards[col][idx]
            updated = {**cards}
            updated[col] = [c for i, c in enumerate(cards[col]) if i != idx]
            updated[target] = [*cards[target], card]
            set_cards(updated)
    
    def add_to_col(col):
        text = inputs[col].strip()
        if text:
            updated = {**cards}
            updated[col] = [*cards[col], inputs[col]]
            set_cards(updated)
            inp_copy = {**inputs}
            inp_copy[col] = ""
            set_inputs(inp_copy)
    
    def render_column(col_id, title):
        cols = ["todo", "doing", "done"]
        idx = cols.index(col_id)
        can_left = idx > 0
        can_right = idx < len(cols) - 1
        
        return div(cn="column",
            h2(title),
            div(cn="cards",
                *[div(cn="card", key=i,
                    span(txt),
                    can_left and button(oc=lambda ci=i, c=col_id: move_card(c, ci, -1), "←"),
                    can_right and button(oc=lambda ci=i, c=col_id: move_card(c, ci, 1), "→"),
                ) for i, txt in enumerate(cards[col_id])]
            ),
            div(cn="add-card",
                input(value=inputs[col_id], oh=lambda e, c=col_id: set_inputs({**inputs, c: e.target.value}), ph=f"Add to {title}..."),
                button(oc=lambda c=col_id: add_to_col(c), "Add"),
            ),
        )
    
    return div(cn="kanban-lite",
        render_column("todo", "Todo"),
        render_column("doing", "Doing"),
        render_column("done", "Done"),
    )
```
