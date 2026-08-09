<!-- exp=baseline-haiku task=macro_kanban_lite cond=ps sample=3 model=claude-haiku-4-5-20251001 cost_usd=0.036397500000000006 -->
```python
"use client"

from pyths.react import component, use_state

@component
def KanbanLite():
    initial_cards = {
        0: [{"id": 1, "title": "Design mockups"}],
        1: [{"id": 2, "title": "Implement feature"}],
        2: [{"id": 3, "title": "Deploy to prod"}]
    }
    cards, set_cards = use_state(initial_cards)
    
    input_values, set_input_values = use_state({0: "", 1: "", 2: ""})
    
    def set_column_input(col_idx, value):
        new_values = {**input_values}
        new_values[col_idx] = value
        set_input_values(new_values)
    
    def add_card(col_idx, title):
        if title.strip():
            new_id = max([c["id"] for col in cards.values() for c in col], default=0) + 1
            new_cards = {**cards}
            new_cards[col_idx] = [*cards[col_idx], {"id": new_id, "title": title}]
            set_cards(new_cards)
            set_column_input(col_idx, "")
    
    def move_card(card_id, from_col, to_col):
        new_cards = {**cards}
        card = None
        for c in cards[from_col]:
            if c["id"] == card_id:
                card = c
                break
        if card:
            new_cards[from_col] = [c for c in cards[from_col] if c["id"] != card_id]
            new_cards[to_col] = [*cards[to_col], card]
            set_cards(new_cards)
    
    def render_card(card, col_idx):
        can_move_left = col_idx > 0
        can_move_right = col_idx < 2
        
        buttons = []
        if can_move_left:
            buttons.append(button(
                on_click=lambda cid=card["id"], fc=col_idx: move_card(cid, fc, fc - 1),
                "←",
                class_name="kanban-move-btn"
            ))
        if can_move_right:
            buttons.append(button(
                on_click=lambda cid=card["id"], fc=col_idx: move_card(cid, fc, fc + 1),
                "→",
                class_name="kanban-move-btn"
            ))
        
        return div(
            class_name="kanban-card",
            div(class_name="kanban-card-title", card["title"]),
            div(class_name="kanban-card-actions", *buttons)
        )
    
    columns = [
        (0, "Todo"),
        (1, "Doing"),
        (2, "Done")
    ]
    
    return div(
        class_name="kanban-container",
        div(
            class_name="kanban-columns",
            *[
                div(
                    class_name="kanban-column",
                    h3(class_name="kanban-column-title", col_name),
                    div(
                        class_name="kanban-column-input",
                        input(
                            value=input_values[col_idx],
                            on_change=lambda e, idx=col_idx: set_column_input(idx, e.target.value),
                            placeholder=f"Add to {col_name}...",
                            class_name="kanban-input"
                        ),
                        button(
                            on_click=lambda idx=col_idx: add_card(idx, input_values[idx]),
                            "Add",
                            class_name="kanban-add-btn"
                        )
                    ),
                    div(
                        class_name="kanban-cards",
                        *[render_card(card, col_idx) for card in cards[col_idx]]
                    )
                )
                for col_idx, col_name in columns
            ]
        )
    )
```
