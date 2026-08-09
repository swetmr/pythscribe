<!-- exp=macrov2-haiku task=macro_todo_list cond=ps sample=3 model=claude-haiku-4-5-20251001 cost_usd=0.014462899999999999 -->
```python
"use client"

from pyths.react import component, use_state

@component
def TodoApp():
    todos, set_todos = use_state([])
    text, set_text = use_state("")

    def add_todo():
        if text.strip():
            set_todos([*todos, {"text": text, "done": False}])
            set_text("")

    def toggle_todo(i):
        updated = [t for t in todos]
        updated[i] = {**updated[i], "done": not updated[i]["done"]}
        set_todos(updated)

    undone_count = len([t for t in todos if not t["done"]])

    return div(class_name="todo-app",
        h1("Todo App"),
        div(class_name="input-section",
            input(
                value=text,
                on_change=lambda e: set_text(e.target.value),
                placeholder="Add a new todo...",
                class_name="todo-input"
            ),
            button(
                on_click=lambda: add_todo(),
                class_name="add-button",
                "Add"
            )
        ),
        ul(class_name="todo-list",
            *[li(
                key=i,
                on_click=lambda: toggle_todo(i),
                class_name="todo-item" + (" done" if t["done"] else ""),
                t["text"]
            ) for i, t in enumerate(todos)]
        ),
        div(class_name="footer",
            p(f"{undone_count} remaining")
        )
    )
```
