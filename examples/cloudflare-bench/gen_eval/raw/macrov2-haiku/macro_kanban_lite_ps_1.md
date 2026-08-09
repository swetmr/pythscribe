<!-- exp=macrov2-haiku task=macro_kanban_lite cond=ps sample=1 model=claude-haiku-4-5-20251001 cost_usd=0.0192796 -->
I'll use the Skill tool to check for applicable skills before building this component.

<tool_use>
{
  "type": "invoke_skill",
  "skill": "frontend-design"
}
</tool_use>

Now I'll build the KanbanLite component based on the requirements:

```python
"use client"

from pyths.react import component, use_state


@component
def KanbanLite():
    columns_state, set_columns_state = use_state({
        "todo": ["Fix login bug", "Write tests"],
        "doing": ["Review PR", "Update docs"],
        "done": ["Deploy to staging", "Update README"],
    })
    
    inputs_state, set_inputs_state = use_state({
        "todo": "",
        "doing": "",
        "done": "",
    })
    
    def move_card(column, card_title, direction):
        column_order = ["todo", "doing", "done"]
        current_idx = column_order.index(column)
        
        if direction == "left" and current_idx > 0:
            next_column = column_order[current_idx - 1]
        elif direction == "right" and current_idx < 2:
            next_column = column_order[current_idx + 1]
        else:
            return
        
        new_columns = {k: list(v) for k, v in columns_state.items()}
        new_columns[column].remove(card_title)
        new_columns[next_column].append(card_title)
        set_columns_state(new_columns)
    
    def add_card(column, title):
        if title.strip():
            new_columns = {k: list(v) for k, v in columns_state.items()}
            new_columns[column].append(title)
            set_columns_state(new_columns)
            new_inputs = {**inputs_state, column: ""}
            set_inputs_state(new_inputs)
    
    def render_column(column_key, column_label):
        column_order = ["todo", "doing", "done"]
        col_idx = column_order.index(column_key)
        
        return div(class_name="column",
            h3(column_label),
            div(class_name="input-group",
                input(
                    type="text",
                    value=inputs_state[column_key],
                    on_change=lambda e: set_inputs_state({
                        **inputs_state,
                        column_key: e.target.value
                    }),
                    placeholder=f"Add {column_label.lower()}...",
                ),
                button(
                    on_click=lambda: add_card(column_key, inputs_state[column_key]),
                    "Add"
                ),
            ),
            div(class_name="cards",
                *[div(class_name="card", key=i,
                    span(card_title),
                    div(class_name="card-buttons",
                        (button(
                            class_name="btn-move",
                            on_click=lambda: move_card(column_key, card_title, "left"),
                            "←"
                        ) if col_idx > 0 else None),
                        (button(
                            class_name="btn-move",
                            on_click=lambda: move_card(column_key, card_title, "right"),
                            "→"
                        ) if col_idx < 2 else None),
                    ),
                ) for i, card_title in enumerate(columns_state[column_key])],
            ),
        )
    
    return div(class_name="kanban-lite",
        h1("Kanban Lite"),
        div(class_name="columns",
            render_column("todo", "Todo"),
            render_column("doing", "Doing"),
            render_column("done", "Done"),
        ),
        style("""
            .kanban-lite { font-family: sans-serif; padding: 20px; }
            .kanban-lite h1 { margin-bottom: 20px; }
            .columns { display: flex; gap: 20px; }
            .column { flex: 1; background: #f5f5f5; padding: 15px; border-radius: 8px; }
            .column h3 { margin-top: 0; }
            .input-group { display: flex; gap: 8px; margin-bottom: 15px; }
            .input-group input { flex: 1; padding: 8px; border: 1px solid #ccc; border-radius: 4px; }
            .input-group button { padding: 8px 12px; background: #007bff; color: white; border: none; border-radius: 4px; cursor: pointer; }
            .input-group button:hover { background: #0056b3; }
            .cards { display: flex; flex-direction: column; gap: 10px; }
            .card { background: white; padding: 12px; border-radius: 4px; box-shadow: 0 2px 4px rgba(0,0,0,0.1); display: flex; justify-content: space-between; align-items: center; }
            .card span { flex: 1; }
            .card-buttons { display: flex; gap: 4px; }
            .btn-move { padding: 4px 8px; background: #6c757d; color: white; border: none; border-radius: 3px; cursor: pointer; font-size: 12px; }
            .btn-move:hover { background: #5a6268; }
        """),
    )
```
