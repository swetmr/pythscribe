<!-- exp=macrov2-haiku task=macro_kanban_lite cond=psc sample=1 model=claude-haiku-4-5-20251001 cost_usd=0.0757846 -->
```python
"use client"
R*

@c
def KanbanLite():
    cards, set_cards = us({
        "todo": ["Design system", "API docs"],
        "doing": ["Implement auth"],
        "done": ["Setup project"]
    })
    
    todo_input, set_todo_input = us("")
    doing_input, set_doing_input = us("")
    done_input, set_done_input = us("")
    
    def move_card(card, from_col, to_col):
        new_cards = {**cards}
        new_cards[from_col] = [c for c in new_cards[from_col] if c != card]
        new_cards[to_col] = [*new_cards[to_col], card]
        set_cards(new_cards)
    
    def add_card(col, text):
        if text.strip():
            new_cards = {**cards}
            new_cards[col] = [*new_cards[col], text]
            set_cards(new_cards)
            if col == "todo":
                set_todo_input("")
            elif col == "doing":
                set_doing_input("")
            else:
                set_done_input("")
    
    def mk_move(c, from_col, to_col):
        return lambda: move_card(c, from_col, to_col)
    
    return div(cn="kanban",
        div(cn="column",
            h3("Todo"),
            div(cn="cards",
                *[div(cn="card",
                    div(cn="card-content", c),
                    div(cn="card-buttons",
                        button(oc=mk_move(c, "todo", "doing"), "→"),
                    ),
                  ) for c in cards["todo"]]
            ),
            div(cn="add-card",
                input(value=todo_input, oh=lambda e: set_todo_input(e.target.value), 
                      ph="New todo..."),
                button(oc=lambda: add_card("todo", todo_input), "Add"),
            ),
        ),
        div(cn="column",
            h3("Doing"),
            div(cn="cards",
                *[div(cn="card",
                    div(cn="card-content", c),
                    div(cn="card-buttons",
                        button(oc=mk_move(c, "doing", "todo"), "←"),
                        button(oc=mk_move(c, "doing", "done"), "→"),
                    ),
                  ) for c in cards["doing"]]
            ),
            div(cn="add-card",
                input(value=doing_input, oh=lambda e: set_doing_input(e.target.value), 
                      ph="New doing..."),
                button(oc=lambda: add_card("doing", doing_input), "Add"),
            ),
        ),
        div(cn="column",
            h3("Done"),
            div(cn="cards",
                *[div(cn="card",
                    div(cn="card-content", c),
                    div(cn="card-buttons",
                        button(oc=mk_move(c, "done", "doing"), "←"),
                    ),
                  ) for c in cards["done"]]
            ),
            div(cn="add-card",
                input(value=done_input, oh=lambda e: set_done_input(e.target.value), 
                      ph="New done..."),
                button(oc=lambda: add_card("done", done_input), "Add"),
            ),
        ),
    )
```
