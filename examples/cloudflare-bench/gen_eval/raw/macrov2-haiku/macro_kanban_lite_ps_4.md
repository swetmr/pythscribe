<!-- exp=macrov2-haiku task=macro_kanban_lite cond=ps sample=4 model=claude-haiku-4-5-20251001 cost_usd=0.07417969999999999 -->
```python
"use client"

from pyths.react import component, use_state

@component
def KanbanLite():
    cards, set_cards = use_state([
        {"id": 1, "title": "Task 1", "column": 0},
        {"id": 2, "title": "Task 2", "column": 0},
        {"id": 3, "title": "Task 3", "column": 1},
        {"id": 4, "title": "Task 4", "column": 2},
    ])
    
    inputs, set_inputs = use_state({"0": "", "1": "", "2": ""})
    next_id, set_next_id = use_state(5)
    
    def add_card(column):
        text = inputs[str(column)]
        if text:
            set_cards([*cards, {"id": next_id, "title": text, "column": column}])
            set_next_id(next_id + 1)
            set_inputs({**inputs, str(column): ""})
    
    def move_card(card_id, direction):
        updated = []
        for card in cards:
            if card["id"] == card_id:
                new_column = card["column"] + direction
                if 0 <= new_column <= 2:
                    updated.append({**card, "column": new_column})
                else:
                    updated.append(card)
            else:
                updated.append(card)
        set_cards(updated)
    
    def mk_input_handler(col):
        return lambda e: set_inputs({**inputs, str(col): e.target.value})
    
    columns = ["Todo", "Doing", "Done"]
    
    return div(class_name="kanban-container",
        *[div(class_name="column",
            h3(columns[i]),
            input(value=inputs[str(i)], 
                  on_change=mk_input_handler(i),
                  placeholder=f"Add to {columns[i]}..."),
            button(on_click=lambda: add_card(i), "Add"),
            div(class_name="cards",
                *[div(class_name="card",
                    (button(on_click=lambda: move_card(c["id"], -1), "←") 
                     if i > 0 else None),
                    span(c["title"]),
                    (button(on_click=lambda: move_card(c["id"], 1), "→") 
                     if i < 2 else None),
                  )
                  for c in [card for card in cards if card["column"] == i]
                ]
            )
          )
          for i in range(3)
        ]
    )
```
