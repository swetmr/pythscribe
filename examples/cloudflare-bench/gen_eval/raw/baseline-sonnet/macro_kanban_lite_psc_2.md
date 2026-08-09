<!-- exp=baseline-sonnet task=macro_kanban_lite cond=psc sample=2 model=claude-sonnet-5 cost_usd=0.1421709 -->
```python
"use client"
R*

COLUMNS = ["Todo", "Doing", "Done"]

def with_key(d, key, value):
    result = {**d}
    result[key] = value
    return result

def render_card(col_idx, card_idx, title, move, can_left, can_right):
    return li(key=card_idx, cn="kanban-card",
        span(cn="kanban-title", title),
        div(cn="kanban-actions",
            can_left and button(cn="kanban-move kanban-move-left", oc=lambda: move(col_idx, card_idx, -1), "<"),
            can_right and button(cn="kanban-move kanban-move-right", oc=lambda: move(col_idx, card_idx, 1), ">"),
        ),
    )

def render_column(col_idx, col, cards, drafts, move, add, set_draft):
    can_left = col_idx > 0
    can_right = col_idx < len(COLUMNS) - 1
    return div(key=col, cn="kanban-column",
        h3(cn="kanban-column-title", col),
        ul(cn="kanban-list",
            *[render_card(col_idx, i, title, move, can_left, can_right)
              for i, title in enumerate(cards[col])],
        ),
        div(cn="kanban-add",
            input(cn="kanban-input", value=drafts[col],
                  oh=lambda e: set_draft(col, e.target.value),
                  ph="New card...",
                  on_key_down=lambda e: add(col) if e.key == "Enter" else None),
            button(cn="kanban-add-button", oc=lambda: add(col), "Add"),
        ),
    )

@c
def KanbanLite():
    cards, set_cards = us({
        "Todo": ["Design mockup", "Research competitors"],
        "Doing": ["Build API"],
        "Done": ["Setup repo"],
    })
    drafts, set_drafts = us({"Todo": "", "Doing": "", "Done": ""})

    def move(col_idx, card_idx, direction):
        target_idx = col_idx + direction
        if target_idx < 0 or target_idx >= len(COLUMNS):
            return
        col = COLUMNS[col_idx]
        target = COLUMNS[target_idx]
        title = cards[col][card_idx]
        new_source = [t for k, t in enumerate(cards[col]) if k != card_idx]
        new_target = [*cards[target], title]
        updated = with_key(cards, col, new_source)
        updated = with_key(updated, target, new_target)
        set_cards(updated)

    def add(col):
        text = drafts[col].strip()
        if not text:
            return
        set_cards(with_key(cards, col, [*cards[col], text]))
        set_drafts(with_key(drafts, col, ""))

    def set_draft(col, value):
        set_drafts(with_key(drafts, col, value))

    return div(cn="kanban-board",
        *[render_column(i, col, cards, drafts, move, add, set_draft)
          for i, col in enumerate(COLUMNS)],
    )
```
