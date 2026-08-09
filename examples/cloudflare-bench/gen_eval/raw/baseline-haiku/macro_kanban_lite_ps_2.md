<!-- exp=baseline-haiku task=macro_kanban_lite cond=ps sample=2 model=claude-haiku-4-5-20251001 cost_usd=0.035237500000000005 -->
```python
"use client"

from pyths.react import component, use_state

@component
def KanbanLite():
    cards, set_cards = use_state({
        "todo": [
            {"id": 1, "title": "Buy milk"},
            {"id": 2, "title": "Write report"}
        ],
        "doing": [
            {"id": 3, "title": "Review PR"}
        ],
        "done": [
            {"id": 4, "title": "Deploy v1.0"}
        ]
    })
    
    inputs, set_inputs = use_state({"todo": "", "doing": "", "done": ""})
    
    columns = ["todo", "doing", "done"]
    column_titles = {"todo": "Todo", "doing": "Doing", "done": "Done"}
    
    def add_card(column):
        text = inputs[column].strip()
        if not text:
            return
        all_ids = [c["id"] for col in cards.values() for c in col]
        new_id = max(all_ids) + 1 if all_ids else 1
        updated_cards = {**cards}
        updated_cards[column] = [*updated_cards[column], {"id": new_id, "title": text}]
        set_cards(updated_cards)
        updated_inputs = {**inputs}
        updated_inputs[column] = ""
        set_inputs(updated_inputs)
    
    def move_card(card_id, from_col, to_col):
        updated = {**cards}
        card = next((c for c in updated[from_col] if c["id"] == card_id), None)
        if card:
            updated[from_col] = [c for c in updated[from_col] if c["id"] != card_id]
            updated[to_col] = [*updated[to_col], card]
            set_cards(updated)
    
    def handle_input_change(column, value):
        updated = {**inputs}
        updated[column] = value
        set_inputs(updated)
    
    return div(class_name="kanban-lite",
        *[div(class_name="kanban-column",
            h3(column_titles[columns[i]]),
            div(class_name="column-input",
                input(
                    value=inputs[columns[i]],
                    on_change=lambda e, col=columns[i]: handle_input_change(col, e.target.value),
                    placeholder=f"Add to {column_titles[columns[i]]}"
                ),
                button(on_click=lambda col=columns[i]: add_card(col), "Add")
            ),
            div(class_name="cards-list",
                *[div(class_name="card",
                    div(class_name="card-title", card["title"]),
                    div(class_name="card-buttons",
                        *(
                            [button(
                                on_click=lambda card_id=card["id"], from_col=columns[i], to_col=columns[i-1]: move_card(card_id, from_col, to_col),
                                class_name="card-button card-button-left",
                                "←"
                            )] if i > 0 else []
                        ),
                        *(
                            [button(
                                on_click=lambda card_id=card["id"], from_col=columns[i], to_col=columns[i+1]: move_card(card_id, from_col, to_col),
                                class_name="card-button card-button-right",
                                "→"
                            )] if i < len(columns) - 1 else []
                        )
                    )
                ) for card in cards[columns[i]]]
            )
        ) for i in range(len(columns))]
    )
```
