<!-- exp=baseline-haiku task=macro_kanban_lite cond=psc sample=2 model=claude-haiku-4-5-20251001 cost_usd=0.0429201 -->
```python
"use client"
R*

@c
def KanbanLite():
    cards, set_cards = us([
        {"id": 0, "title": "Plan sprint", "column": 0},
        {"id": 1, "title": "Build API", "column": 1},
        {"id": 2, "title": "Deploy", "column": 2},
    ])
    next_id, set_next_id = us(3)
    
    input_todo, set_input_todo = us("")
    input_doing, set_input_doing = us("")
    input_done, set_input_done = us("")
    
    def move_card(card_id, direction):
        updated = []
        for c in cards:
            if c["id"] == card_id:
                new_col = c["column"] + direction
                updated.append({**c, "column": new_col})
            else:
                updated.append(c)
        set_cards(updated)
    
    def add_card(column, text, set_text):
        if text.strip():
            set_cards([*cards, {"id": next_id, "title": text, "column": column}])
            set_next_id(next_id + 1)
            set_text("")
    
    def column_cards(col_idx):
        return [c for c in cards if c["column"] == col_idx]
    
    def render_card(card, col_idx):
        buttons = []
        if col_idx > 0:
            buttons.append(button(cn="btn-left",
                                 oc=lambda cid=card["id"]: move_card(cid, -1),
                                 "←"))
        if col_idx < 2:
            buttons.append(button(cn="btn-right",
                                 oc=lambda cid=card["id"]: move_card(cid, 1),
                                 "→"))
        
        return div(cn="card", key=card["id"],
            div(cn="card-content", card["title"]),
            div(cn="card-buttons", *buttons),
        )
    
    def render_column(title, col_idx, input_state, set_input):
        cards_in_col = column_cards(col_idx)
        return div(cn="column",
            h2(title),
            div(cn="column-input",
                input(value=input_state, oh=lambda e: set_input(e.target.value), ph="Enter task..."),
                button(oc=lambda: add_card(col_idx, input_state, set_input), "Add"),
            ),
            div(cn="cards",
                *[render_card(c, col_idx) for c in cards_in_col],
            ),
        )
    
    return div(cn="kanban-lite",
        div(cn="columns",
            render_column("Todo", 0, input_todo, set_input_todo),
            render_column("Doing", 1, input_doing, set_input_doing),
            render_column("Done", 2, input_done, set_input_done),
        ),
    )
```
