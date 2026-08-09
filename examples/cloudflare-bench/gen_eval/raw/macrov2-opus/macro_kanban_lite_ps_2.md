<!-- exp=macrov2-opus task=macro_kanban_lite cond=ps sample=2 model=claude-opus-4-8 cost_usd=0.03994399999999999 -->
```python
"use client"

from pyths.react import component, use_state

COLUMNS = ["Todo", "Doing", "Done"]

INITIAL_CARDS = {
    "Todo": ["Write spec", "Sketch UI"],
    "Doing": ["Build API"],
    "Done": ["Set up repo"],
}


@component
def KanbanLite():
    cards, set_cards = use_state(INITIAL_CARDS)
    drafts, set_drafts = use_state({"Todo": "", "Doing": "", "Done": ""})

    def move(col_index, card_index, direction):
        target_index = col_index + direction
        if target_index < 0 or target_index >= len(COLUMNS):
            return
        from_col = COLUMNS[col_index]
        to_col = COLUMNS[target_index]
        next_cards = {k: [c for c in v] for k, v in cards.items()}
        title = next_cards[from_col].pop(card_index)
        next_cards[to_col].append(title)
        set_cards(next_cards)

    def set_draft(col, value):
        set_drafts({**drafts, col: value})

    def add(col):
        title = drafts[col].strip()
        if not title:
            return
        next_cards = {k: [c for c in v] for k, v in cards.items()}
        next_cards[col].append(title)
        set_cards(next_cards)
        set_draft(col, "")

    def render_card(col_index, card_index, title):
        col = COLUMNS[col_index]
        can_left = col_index > 0
        can_right = col_index < len(COLUMNS) - 1
        return li(key=f"{col}-{card_index}-{title}", class_name="kanban-card",
            span(class_name="kanban-card-title", title),
            div(class_name="kanban-card-actions",
                button(class_name="kanban-move kanban-move-left",
                       on_click=lambda: move(col_index, card_index, -1),
                       "\u2190") if can_left else None,
                button(class_name="kanban-move kanban-move-right",
                       on_click=lambda: move(col_index, card_index, 1),
                       "\u2192") if can_right else None,
            ),
        )

    def render_column(col_index, col):
        column_cards = cards[col]
        return section(key=col, class_name="kanban-column",
            h2(class_name="kanban-column-title", col),
            ul(class_name="kanban-list",
                *[render_card(col_index, i, title)
                  for i, title in enumerate(column_cards)]),
            div(class_name="kanban-add",
                input(class_name="kanban-input", value=drafts[col],
                      placeholder=f"Add to {col}...",
                      on_change=lambda e: set_draft(col, e.target.value)),
                button(class_name="kanban-add-btn",
                       on_click=lambda: add(col), "Add"),
            ),
        )

    return div(class_name="kanban-board",
        h1(class_name="kanban-heading", "KanbanLite"),
        div(class_name="kanban-columns",
            *[render_column(i, col) for i, col in enumerate(COLUMNS)]),
    )
```
