<!-- exp=baseline-haiku task=macro_kanban_lite cond=psc sample=1 model=claude-haiku-4-5-20251001 cost_usd=0.052783300000000005 -->
```python
"use client"
R*

@c
def KanbanLite():
    cards, set_cards = us({
        "todo": ["Design homepage", "Fix bug #42"],
        "doing": ["Implement auth"],
        "done": ["Setup database"]
    })
    
    todo_input, set_todo_input = us("")
    doing_input, set_doing_input = us("")
    done_input, set_done_input = us("")
    
    def add_card(column, title):
        if title.strip():
            new_cards = {**cards}
            new_cards[column] = [*new_cards[column], title]
            set_cards(new_cards)
            if column == "todo":
                set_todo_input("")
            elif column == "doing":
                set_doing_input("")
            else:
                set_done_input("")
    
    def move_card(column, index, direction):
        columns = ["todo", "doing", "done"]
        col_idx = columns.index(column)
        new_col_idx = col_idx + direction
        if 0 <= new_col_idx < len(columns):
            new_cards = {**cards}
            card = new_cards[column].pop(index)
            new_cards[columns[new_col_idx]].append(card)
            set_cards(new_cards)
    
    def render_column(column, title):
        col_cards = cards[column]
        if column == "todo":
            input_val, set_input = todo_input, set_todo_input
        elif column == "doing":
            input_val, set_input = doing_input, set_doing_input
        else:
            input_val, set_input = done_input, set_done_input
        
        return div(cn="column",
            h2(title),
            div(cn="cards",
                *[div(cn="card", key=i,
                    div(cn="card-title", card),
                    div(cn="card-actions",
                        button(oc=lambda idx=i: move_card(column, idx, -1), "←") if column != "todo" else None,
                        button(oc=lambda idx=i: move_card(column, idx, 1), "→") if column != "done" else None,
                    )
                ) for i, card in enumerate(col_cards)]
            ),
            div(cn="add-section",
                input(value=input_val, oh=lambda e: set_input(e.target.value), ph=f"Add to {title}"),
                button(oc=lambda: add_card(column, input_val), "Add"),
            )
        )
    
    return div(cn="kanban",
        render_column("todo", "Todo"),
        render_column("doing", "Doing"),
        render_column("done", "Done"),
    )
```
