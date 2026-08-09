<!-- exp=macrov2-haiku task=macro_kanban_lite cond=psc sample=4 model=claude-haiku-4-5-20251001 cost_usd=0.08106419999999999 -->
```python
R*

@c
def KanbanLite():
    cards, set_cards = us({
        "todo": [{"id": 1, "title": "Design mockups"}, {"id": 2, "title": "Review requirements"}],
        "doing": [{"id": 3, "title": "Build UI"}],
        "done": [{"id": 4, "title": "Setup project"}]
    })
    
    todo_input, set_todo_input = us("")
    doing_input, set_doing_input = us("")
    done_input, set_done_input = us("")
    
    next_id, set_next_id = us(5)
    
    def add_card(column, text, clear_fn):
        if text.strip():
            new_cards = {**cards}
            new_cards[column] = [*new_cards[column], {"id": next_id, "title": text}]
            set_cards(new_cards)
            set_next_id(next_id + 1)
            clear_fn("")
    
    def move_card(card_id, from_col, to_col):
        new_cards = {**cards}
        card = next((c for c in new_cards[from_col] if c["id"] == card_id), None)
        if card:
            new_cards[from_col] = [c for c in new_cards[from_col] if c["id"] != card_id]
            new_cards[to_col] = [*new_cards[to_col], card]
            set_cards(new_cards)
    
    def mk_move(cid, fc, tc):
        return lambda: move_card(cid, fc, tc)
    
    @c
    def Col(name, key, inp, set_inp):
        prev_col = {"todo": None, "doing": "todo", "done": "doing"}[key]
        next_col = {"todo": "doing", "doing": "done", "done": None}[key]
        
        return div(cn="col",
            h3(name),
            div(*[div(cn="card", key=c["id"],
                      div(cn="title", c["title"]),
                      div(cn="btns",
                          button(cn="btn", oc=mk_move(c["id"], key, prev_col), "←") if prev_col else None,
                          button(cn="btn", oc=mk_move(c["id"], key, next_col), "→") if next_col else None,
                      ),
                  ) for c in cards[key]]),
            div(cn="add",
                input(ph=f"Add {name.lower()}...", value=inp, oh=lambda e: set_inp(e.target.value)),
                button(oc=lambda: add_card(key, inp, set_inp), "Add"),
            ),
        )
    
    return div(cn="kanban",
        Col("Todo", "todo", todo_input, set_todo_input),
        Col("Doing", "doing", doing_input, set_doing_input),
        Col("Done", "done", done_input, set_done_input),
    )
```
