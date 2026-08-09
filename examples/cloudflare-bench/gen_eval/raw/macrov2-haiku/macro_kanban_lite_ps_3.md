<!-- exp=macrov2-haiku task=macro_kanban_lite cond=ps sample=3 model=claude-haiku-4-5-20251001 cost_usd=0.0648947 -->
```python
from pyths.react import component, use_state

@component
def KanbanLite():
    cards, set_cards = use_state([
        [
            {"id": 1, "title": "Design wireframes"},
            {"id": 2, "title": "Setup project"},
        ],
        [
            {"id": 3, "title": "Build components"},
        ],
        [
            {"id": 4, "title": "Write tests"},
        ],
    ])
    next_id, set_next_id = use_state(5)
    inputs, set_inputs = use_state(["", "", ""])
    
    def move_card(from_col, card_index, direction):
        to_col = from_col + direction
        if to_col < 0 or to_col > 2:
            return
        new_cards = [list(col) for col in cards]
        card = new_cards[from_col].pop(card_index)
        new_cards[to_col].append(card)
        set_cards(new_cards)
    
    def add_card(col_index):
        text = inputs[col_index].strip()
        if not text:
            return
        new_cards = [list(col) for col in cards]
        new_cards[col_index].append({"id": next_id, "title": text})
        set_cards(new_cards)
        new_inputs = list(inputs)
        new_inputs[col_index] = ""
        set_inputs(new_inputs)
        set_next_id(next_id + 1)
    
    def update_input(col_index, value):
        new_inputs = list(inputs)
        new_inputs[col_index] = value
        set_inputs(new_inputs)
    
    columns = ["Todo", "Doing", "Done"]
    
    return div(class_name="kanban-container",
        div(class_name="kanban-board",
            *[
                div(class_name="kanban-column",
                    h3(columns[i]),
                    div(class_name="column-input-area",
                        input(
                            type="text",
                            value=inputs[i],
                            on_change=lambda e, col=i: update_input(col, e.target.value),
                            placeholder=f"New {columns[i].lower()}...",
                            class_name="column-input",
                        ),
                        button(
                            on_click=lambda col=i: add_card(col),
                            class_name="add-button",
                            "Add",
                        ),
                    ),
                    div(class_name="cards-list",
                        *[
                            div(class_name="card",
                                div(class_name="card-content",
                                    span(class_name="card-title", card["title"]),
                                    div(class_name="card-buttons",
                                        button(
                                            on_click=lambda: move_card(i, idx, -1),
                                            class_name="move-button",
                                            "←",
                                        ) if i > 0 else None,
                                        button(
                                            on_click=lambda: move_card(i, idx, 1),
                                            class_name="move-button",
                                            "→",
                                        ) if i < 2 else None,
                                    ),
                                ),
                            )
                            for idx, card in enumerate(cards[i])
                        ],
                    ),
                )
                for i in range(3)
            ],
        ),
    )
```
