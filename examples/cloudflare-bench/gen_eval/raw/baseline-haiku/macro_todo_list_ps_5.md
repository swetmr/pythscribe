<!-- exp=baseline-haiku task=macro_todo_list cond=ps sample=5 model=claude-haiku-4-5-20251001 cost_usd=0.0192807 -->
```python
"use client"

from pyths.react import component, use_state

@component
def TodoApp():
    todos, set_todos = use_state([])
    input_text, set_input_text = use_state("")

    def add_todo():
        if input_text.strip():
            new_todo = {"id": len(todos), "text": input_text, "done": False}
            set_todos([*todos, new_todo])
            set_input_text("")

    def toggle_todo(todo_id):
        updated = [
            {**todo, "done": not todo["done"]} if todo["id"] == todo_id else todo
            for todo in todos
        ]
        set_todos(updated)

    remaining = sum(1 for todo in todos if not todo["done"])

    return div(class_name="app",
        h1("Todos"),
        input(
            value=input_text,
            on_change=lambda e: set_input_text(e.target.value),
            placeholder="Add a new todo",
            class_name="input"
        ),
        button(on_click=lambda: add_todo(), class_name="btn", "Add"),
        ul(class_name="list",
            *[li(
                key=todo["id"],
                on_click=lambda tid=todo["id"]: toggle_todo(tid),
                class_name="todo" if not todo["done"] else "todo done",
                todo["text"]
            ) for todo in todos]
        ),
        p(class_name="footer", f"{remaining} remaining")
    )
```
