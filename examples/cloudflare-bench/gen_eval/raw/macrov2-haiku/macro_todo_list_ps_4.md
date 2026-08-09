<!-- exp=macrov2-haiku task=macro_todo_list cond=ps sample=4 model=claude-haiku-4-5-20251001 cost_usd=0.023527899999999997 -->
```python
"use client"

from pyths.react import component, use_state

@component
def TodoApp():
    todos, set_todos = use_state([])
    text, set_text = use_state("")

    def add():
        if text.strip():
            set_todos([*todos, {"text": text, "done": False}])
            set_text("")

    def toggle(i):
        updated = [t for t in todos]
        updated[i] = {**updated[i], "done": not updated[i]["done"]}
        set_todos(updated)

    pending = len([t for t in todos if not t["done"]])

    return div(class_name="todo-app",
        h1("Todo App"),
        div(class_name="input-container",
            input(
                value=text,
                on_change=lambda e: set_text(e.target.value),
                placeholder="Add a new todo...",
                type="text"
            ),
            button(on_click=lambda: add(), "Add")
        ),
        ul(class_name="todo-list",
            *[li(
                key=i,
                class_name=f"todo-item{' done' if t['done'] else ''}",
                on_click=lambda: toggle(i),
                t["text"]
            ) for i, t in enumerate(todos)]
        ),
        footer(class_name="todo-footer",
            p(f"{pending} remaining")
        )
    )
```
