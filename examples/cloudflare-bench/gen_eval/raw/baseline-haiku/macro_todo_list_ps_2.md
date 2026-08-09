<!-- exp=baseline-haiku task=macro_todo_list cond=ps sample=2 model=claude-haiku-4-5-20251001 cost_usd=0.0326157 -->
```python
"use client"

from pyths.react import component, use_state

@component
def TodoApp():
    todos, set_todos = use_state([])
    text, set_text = use_state("")

    def add():
        stripped = text.strip()
        if stripped:
            set_todos([*todos, {"text": stripped, "done": False}])
            set_text("")

    def toggle(i):
        updated = [t for t in todos]
        updated[i] = {**updated[i], "done": not updated[i]["done"]}
        set_todos(updated)

    remaining = len([t for t in todos if not t["done"]])

    return div(class_name="todo-app",
        h1("Todo List"),
        div(class_name="todo-input-container",
            input(value=text, on_change=lambda e: set_text(e.target.value),
                  placeholder="Add a new todo..."),
            button(on_click=add, "Add"),
        ),
        ul(class_name="todo-list",
            *[li(key=i,
                 on_click=lambda i=i: toggle(i),
                 class_name="todo-item" + (" done" if t["done"] else ""),
                 t["text"])
              for i, t in enumerate(todos)],
        ) if todos else None,
        div(class_name="todo-footer",
            p(f"{remaining} remaining"),
        ),
    )
```
