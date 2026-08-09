<!-- exp=macrov2-haiku task=macro_kanban_lite cond=ps sample=5 model=claude-haiku-4-5-20251001 cost_usd=0.033364700000000004 -->
```python
"use client"

from pyths.react import component, use_state

@component
def KanbanLite():
    cards, set_cards = use_state({
        "todo": [
            {"id": 1, "title": "Design homepage"},
            {"id": 2, "title": "Setup database"},
            {"id": 3, "title": "Create API docs"},
        ],
        "doing": [
            {"id": 4, "title": "Implement auth"},
            {"id": 5, "title": "Database migration"},
        ],
        "done": [
            {"id": 6, "title": "Project setup"},
            {"id": 7, "title": "Initial design"},
        ],
    })
    
    next_id, set_next_id = use_state(8)
    
    columns = ["todo", "doing", "done"]
    column_names = {"todo": "Todo", "doing": "Doing", "done": "Done"}
    
    def add_card(col, title):
        if title.strip():
            new_cards = {**cards}
            new_cards[col] = [*cards[col], {"id": next_id, "title": title}]
            set_cards(new_cards)
            set_next_id(next_id + 1)
    
    def move_card(card_id, from_col, to_col):
        card = next((c for c in cards[from_col] if c["id"] == card_id), None)
        if card:
            new_cards = {**cards}
            new_cards[from_col] = [c for c in cards[from_col] if c["id"] != card_id]
            new_cards[to_col] = [*cards[to_col], card]
            set_cards(new_cards)
    
    return div(class_name="kanban-container",
        h1("Kanban Board"),
        div(class_name="kanban-board",
            *[Column(col, column_names[col], cards[col], move_card, add_card)
              for col in columns]
        )
    )

@component
def Column(col, title, cards_list, move_card, add_card):
    input_val, set_input_val = use_state("")
    
    def handle_add():
        add_card(col, input_val)
        set_input_val("")
    
    return div(class_name="kanban-column",
        h3(title),
        div(class_name="kanban-input-group",
            input(
                value=input_val,
                on_change=lambda e: set_input_val(e.target.value),
                placeholder="Add a card...",
                class_name="kanban-input"
            ),
            button(on_click=handle_add, "Add", class_name="kanban-add-btn"),
        ),
        div(class_name="kanban-cards",
            *[Card(card, col, move_card) for card in cards_list]
        )
    )

@component
def Card(card, current_col, move_card):
    columns = ["todo", "doing", "done"]
    col_index = columns.index(current_col)
    
    return div(class_name="kanban-card",
        p(class_name="card-title", card["title"]),
        div(class_name="card-buttons",
            button(
                on_click=lambda: move_card(card["id"], current_col, columns[col_index - 1]),
                "←",
                class_name="card-btn card-btn-left"
            ) if col_index > 0 else None,
            button(
                on_click=lambda: move_card(card["id"], current_col, columns[col_index + 1]),
                "→",
                class_name="card-btn card-btn-right"
            ) if col_index < len(columns) - 1 else None,
        )
    )
```
